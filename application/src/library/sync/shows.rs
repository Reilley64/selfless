use crate::library::Prober;
use crate::library::library_items;
use crate::library::library_items::Change;
use crate::library::media_files;
use crate::library::media_files::Inspected;
use crate::library::metadata;
use crate::library::metadata::FoundArtwork;
use crate::library::sync::Error;
use crate::ports::EpisodeFolder;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ShowFolder;
use crate::ports::SourceEpisode;
use crate::ports::SourceShow;
use crate::ports::UnreadableFile;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::ArtworkKind;
use domain::Episode;
use domain::EpisodeMetadata;
use domain::EpisodeNfo;
use domain::MediaFile;
use domain::Nfo;
use domain::Owner;
use domain::Season;
use domain::Show;
use domain::ShowMetadata;
use domain::ShowNfo;
use domain::SonarrId;
use domain::Source;
use futures_util::future::try_join_all;
use jiff::Timestamp;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::Span;
use uuid::Uuid;

pub(crate) struct ShowFiles {
    pub(crate) find_media_file: FindMediaFile,
    pub(crate) list_subtitle_files: ListSubtitleFiles,
    pub(crate) prober: Prober,
    pub(crate) read_episode_folder: ReadEpisodeFolder,
    pub(crate) read_episode_nfo: ReadEpisodeNfo,
    pub(crate) read_show_folder: ReadShowFolder,
    pub(crate) read_show_nfo: ReadShowNfo,
}

pub(crate) fn span() -> Span {
    tracing::info_span!("sync_show", otel.name = "sync sonarr show")
}

pub(crate) struct ShowOnDisk {
    episode_folders: HashMap<String, EpisodeFolder>,
    media_files: HashMap<String, Inspected>,
    folder: ShowFolder,
    nfo: Nfo<ShowNfo>,
    episode_nfos: Vec<Nfo<EpisodeNfo>>,
}

pub(crate) enum InspectedShow {
    Read(Box<ShowOnDisk>),
    Unreadable(UnreadableFile),
}

impl InspectedShow {
    pub(crate) fn unreadable(&self) -> bool {
        matches!(self, InspectedShow::Unreadable(_))
    }
}

pub(crate) async fn inspect(
    db_context: &mut DbContext,
    files: &ShowFiles,
    source_show: &SourceShow,
) -> Result<InspectedShow, Error> {
    let media_file_paths = source_show
        .episodes
        .iter()
        .filter_map(|x| x.media_file.as_ref())
        .map(|x| x.path.clone())
        .collect::<Vec<_>>();
    let stored = if media_file_paths.is_empty() {
        HashMap::new()
    } else {
        db_context
            .media_files()
            .filter(MediaFile::fields().path().in_list(media_file_paths))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.path.clone(), x))
            .collect::<HashMap<_, _>>()
    };
    let stored_show = db_context
        .shows()
        .filter(Show::fields().tvdb_id().eq(source_show.listing.tvdb_id))
        .include(Show::fields().seasons().episodes())
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let stored_nfo_modified_at = stored_show
        .iter()
        .flat_map(|x| x.seasons.get())
        .flat_map(|x| x.episodes.get())
        .map(|x| (x.tvdb_id, x.nfo_modified_at))
        .collect::<HashMap<_, _>>();
    let sonarr_ids = source_show
        .episodes
        .iter()
        .map(|x| x.sonarr_id)
        .collect::<HashSet<_>>();
    let read = async {
        let mut found = HashMap::new();
        let mut episode_folders = HashMap::new();
        for file in source_show
            .episodes
            .iter()
            .filter_map(|x| x.media_file.as_ref())
        {
            if !episode_folders.contains_key(&file.path) {
                found.insert(
                    file.path.clone(),
                    files.find_media_file.call(&file.path).await?,
                );
                episode_folders.insert(
                    file.path.clone(),
                    files.read_episode_folder.call(&file.path).await?,
                );
            }
        }
        let season_numbers = source_show
            .episodes
            .iter()
            .map(|x| x.season_number)
            .collect::<BTreeSet<_>>();
        let mut folder = files
            .read_show_folder
            .call(
                &source_show.listing.path,
                season_numbers.into_iter().collect(),
            )
            .await?;
        let nfo = metadata::read_nfo(
            folder.nfo.take(),
            stored_show.as_ref().and_then(|x| x.nfo_modified_at),
            |path| files.read_show_nfo.call(path),
        )
        .await?;
        let nfo_file = |source_episode: &SourceEpisode| {
            source_episode
                .media_file
                .as_ref()
                .and_then(|x| episode_folders.get(&x.path))
                .and_then(|x| x.nfo.as_ref())
        };
        let mut nfo_files = BTreeMap::new();
        for source_episode in &source_show.episodes {
            if let Some(file) = nfo_file(source_episode) {
                let stored = stored_nfo_modified_at
                    .get(&source_episode.tvdb_id)
                    .copied()
                    .flatten();
                nfo_files
                    .entry(file.path.clone())
                    .and_modify(|(_, x)| {
                        if *x != stored {
                            *x = None;
                        }
                    })
                    .or_insert((file.clone(), stored));
            }
        }
        let mut nfos = HashMap::new();
        for (path, (file, stored)) in nfo_files {
            let nfo =
                metadata::read_nfo(Some(file), stored, |path| files.read_episode_nfo.call(path))
                    .await?;
            nfos.insert(path, nfo);
        }
        let episode_nfos = source_show
            .episodes
            .iter()
            .map(
                |source_episode| match nfo_file(source_episode).map(|x| &nfos[&x.path]) {
                    None | Some(Nfo::Missing) => Nfo::Missing,
                    Some(Nfo::Unchanged) => Nfo::Unchanged,
                    Some(Nfo::Unreadable(path)) => Nfo::Unreadable(path.clone()),
                    Some(Nfo::Read(entries, modified_at)) => match EpisodeNfo::matching(
                        entries,
                        source_episode.sonarr_id,
                        source_episode.season_number,
                        source_episode.episode_number,
                        &sonarr_ids,
                    ) {
                        Some(entry) => Nfo::Read(entry.clone(), *modified_at),
                        None => Nfo::Missing,
                    },
                },
            )
            .collect::<Vec<_>>();
        Ok::<_, UnreadableFile>((found, episode_folders, folder, nfo, episode_nfos))
    };
    let (found, episode_folders, folder, nfo, episode_nfos) = match read.await {
        Ok(read) => read,
        Err(error) => {
            metadata::warn_unreadable_file(&error);
            return Ok(InspectedShow::Unreadable(error));
        }
    };
    let media_files = try_join_all(found.into_iter().map(async |(path, found)| {
        let inspected = media_files::inspect(
            &files.list_subtitle_files,
            &files.prober,
            stored.get(&path),
            &path,
            found,
        )
        .await?;
        Ok::<_, media_files::Error>((path, inspected))
    }))
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .collect::<HashMap<_, _>>();
    Ok(InspectedShow::Read(Box::new(ShowOnDisk {
        episode_folders,
        media_files,
        folder,
        nfo,
        episode_nfos,
    })))
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    source_show: SourceShow,
    inspected: InspectedShow,
) -> Result<(), Error> {
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let change = match inspected {
        InspectedShow::Read(on_disk) => {
            write(&mut transaction, source_show, *on_disk, None).await?
        }
        InspectedShow::Unreadable(error) => match transaction
            .shows()
            .filter(Show::fields().tvdb_id().eq(source_show.listing.tvdb_id))
            .first()
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?
        {
            Some(show) => {
                metadata::save_unreadable_file(
                    &mut transaction,
                    Owner::Show(show.id),
                    Some(error.path),
                )
                .await
                .map_err(Error::caused_by)?;
                None
            }
            None => {
                let on_disk = ShowOnDisk {
                    episode_folders: HashMap::new(),
                    media_files: HashMap::new(),
                    folder: ShowFolder::default(),
                    nfo: Nfo::Missing,
                    episode_nfos: source_show.episodes.iter().map(|_| Nfo::Missing).collect(),
                };
                let source_show = SourceShow {
                    episodes: source_show
                        .episodes
                        .into_iter()
                        .map(|x| SourceEpisode {
                            media_file: None,
                            ..x
                        })
                        .collect(),
                    ..source_show
                };
                write(&mut transaction, source_show, on_disk, Some(error.path)).await?
            }
        },
    };
    transaction.commit().await.map_err(Error::caused_by)?;
    if let Some(change) = change {
        library_items::count(Source::Sonarr, change);
    }
    Ok(())
}

pub(crate) async fn remove(db_context: &mut DbContext, show: Show) -> Result<(), Error> {
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let media_file_ids = show
        .seasons()
        .include(Season::fields().episodes())
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .flat_map(|x| x.episodes.into_inner())
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    metadata::delete(&mut transaction, Owner::Show(show.id))
        .await
        .map_err(Error::caused_by)?;
    show.delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    media_files::delete(&mut transaction, media_file_ids.into_iter().collect())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    library_items::count(Source::Sonarr, Change::Removed);
    Ok(())
}

struct SyncedShow {
    sonarr_id: SonarrId,
    year: Option<i64>,
    metadata: ShowMetadata,
    nfo_modified_at: Option<Timestamp>,
}

macro_rules! set_show {
    ($builder:expr, $synced:expr) => {{
        let SyncedShow {
            sonarr_id,
            year,
            metadata,
            nfo_modified_at,
        } = $synced;
        $builder
            .sonarr_id(sonarr_id)
            .year(year)
            .title(metadata.title)
            .sort_title(metadata.sort_title)
            .overview(metadata.overview)
            .certification(metadata.certification)
            .premiered(metadata.premiered)
            .end_date(metadata.end_date)
            .status(metadata.status)
            .network(metadata.network)
            .genres(metadata.genres)
            .rating(metadata.rating)
            .nfo_modified_at(nfo_modified_at)
    }};
}

struct SyncedEpisode {
    season_id: Uuid,
    number: i64,
    metadata: EpisodeMetadata,
    nfo_modified_at: Option<Timestamp>,
    added_at: Option<Timestamp>,
    media_file_id: Option<Uuid>,
    in_library: bool,
}

macro_rules! set_episode {
    ($builder:expr, $synced:expr) => {{
        let SyncedEpisode {
            season_id,
            number,
            metadata,
            nfo_modified_at,
            added_at,
            media_file_id,
            in_library,
        } = $synced;
        $builder
            .season_id(season_id)
            .number(number)
            .title(metadata.title)
            .air_date(metadata.air_date)
            .overview(metadata.overview)
            .rating(metadata.rating)
            .nfo_modified_at(nfo_modified_at)
            .added_at(added_at)
            .media_file_id(media_file_id)
            .in_library(in_library)
    }};
}

async fn write(
    scope: &mut impl DbScope,
    source_show: SourceShow,
    on_disk: ShowOnDisk,
    unreadable: Option<String>,
) -> Result<Option<Change>, Error> {
    let ShowOnDisk {
        episode_folders,
        media_files: mut inspected,
        folder,
        nfo,
        episode_nfos,
    } = on_disk;
    let listing = source_show.listing;
    let stored = scope
        .shows()
        .filter(Show::fields().tvdb_id().eq(listing.tvdb_id))
        .first()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let had_nfo = stored.as_ref().is_some_and(|x| x.nfo_modified_at.is_some());
    let source_metadata =
        ShowMetadata::from_source(listing.title, listing.overview, listing.genres);
    let (metadata, nfo_modified_at) = nfo.metadata(
        stored.as_ref().map(|x| (x.metadata(), x.nfo_modified_at)),
        source_metadata,
        ShowMetadata::with_nfo,
    );
    let synced = SyncedShow {
        sonarr_id: listing.sonarr_id,
        year: listing.year,
        metadata,
        nfo_modified_at,
    };
    let (show, show_change) = match stored {
        Some(mut show) => {
            let changed = show.sonarr_id != synced.sonarr_id
                || show.year != synced.year
                || show.nfo_modified_at != synced.nfo_modified_at
                || show.metadata() != synced.metadata;
            if changed {
                set_show!(show.update(), synced)
                    .exec(scope.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
            }
            (show, changed.then_some(Change::Changed))
        }
        None => (
            set_show!(scope.shows().create().tvdb_id(listing.tvdb_id), synced)
                .exec(scope.executor_mut())
                .await
                .map_err(Error::caused_by)?,
            Some(Change::Added),
        ),
    };
    let owner = Owner::Show(show.id);
    metadata::save_actors(scope, owner, had_nfo, &nfo, |x| &x.actors)
        .await
        .map_err(Error::caused_by)?;
    let mut changed = show_change.is_some();
    let mut artwork = metadata::item_artwork(folder.poster, folder.fanart, folder.clearlogo);
    let mut seasons = HashMap::new();
    let mut episodes = HashMap::new();
    for season in show
        .seasons()
        .include(Season::fields().episodes())
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?
    {
        seasons.insert(season.number, season.id);
        episodes.extend(
            season
                .episodes
                .into_inner()
                .into_iter()
                .map(|x| (x.tvdb_id, x)),
        );
    }
    let old_media_files = episodes
        .values()
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    let mut synced_seasons = HashSet::new();
    let mut media_file_ids = HashSet::new();
    let mut unreadable_nfos = HashMap::new();
    let mut saved = HashMap::new();
    for (source_episode, nfo) in source_show.episodes.into_iter().zip(episode_nfos) {
        let episode_folder = source_episode
            .media_file
            .as_ref()
            .and_then(|x| episode_folders.get(&x.path));
        let stored = episodes.remove(&source_episode.tvdb_id);
        let source_metadata = EpisodeMetadata::from_source(
            source_episode.title,
            source_episode.air_date,
            source_episode.overview,
        );
        let (metadata, nfo_modified_at) = nfo.metadata(
            stored.as_ref().map(|x| (x.metadata(), x.nfo_modified_at)),
            source_metadata,
            EpisodeMetadata::with_nfo,
        );
        let season_number = source_episode.season_number;
        let season_id = match seasons.get(&season_number) {
            Some(id) => *id,
            None => {
                let season = scope
                    .seasons()
                    .create()
                    .show_id(show.id)
                    .number(season_number)
                    .exec(scope.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
                changed = true;
                seasons.insert(season_number, season.id);
                season.id
            }
        };
        synced_seasons.insert(season_number);
        let (media_file_id, file_on_disk) = match &source_episode.media_file {
            Some(source_file) => {
                let (id, on_disk) = match inspected.remove(&source_file.path) {
                    Some(inspected) => {
                        let on_disk = inspected.on_disk();
                        let saved_file = media_files::save(scope, &source_file.path, inspected)
                            .await
                            .map_err(Error::caused_by)?;
                        changed |= saved_file.changed;
                        saved.insert(source_file.path.clone(), (saved_file.id, on_disk));
                        (saved_file.id, on_disk)
                    }
                    None => saved[&source_file.path],
                };
                media_file_ids.insert(id);
                if let Nfo::Unreadable(path) = &nfo {
                    unreadable_nfos.insert(id, path.clone());
                }
                (Some(id), on_disk)
            }
            None => (None, false),
        };
        let in_library = Episode::in_library(source_episode.media_file.is_some(), file_on_disk);
        let source_added_at = source_episode.media_file.map(|x| x.added_at);
        let synced = SyncedEpisode {
            season_id,
            number: source_episode.episode_number,
            metadata,
            nfo_modified_at,
            added_at: domain::first_added_at(
                stored.as_ref().and_then(|x| x.added_at),
                source_added_at,
            ),
            media_file_id,
            in_library,
        };
        let episode_id = match stored {
            Some(mut episode) => {
                if episode.season_id != synced.season_id
                    || episode.number != synced.number
                    || episode.nfo_modified_at != synced.nfo_modified_at
                    || episode.metadata() != synced.metadata
                    || episode.added_at != synced.added_at
                    || episode.media_file_id != synced.media_file_id
                    || episode.in_library != synced.in_library
                {
                    changed = true;
                    set_episode!(episode.update(), synced)
                        .exec(scope.executor_mut())
                        .await
                        .map_err(Error::caused_by)?;
                }
                episode.id
            }
            None => {
                changed = true;
                set_episode!(
                    scope.episodes().create().tvdb_id(source_episode.tvdb_id),
                    synced
                )
                .exec(scope.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .id
            }
        };
        if let Some(thumb) = episode_folder.and_then(|x| x.thumb.clone()) {
            artwork.push(FoundArtwork {
                season_id: None,
                episode_id: Some(episode_id),
                kind: ArtworkKind::Thumb,
                file: thumb,
            });
        }
    }
    for (number, file) in folder.season_posters {
        if let Some(id) = seasons.get(&number)
            && synced_seasons.contains(&number)
        {
            artwork.push(FoundArtwork {
                season_id: Some(*id),
                episode_id: None,
                kind: ArtworkKind::Poster,
                file,
            });
        }
    }
    let episodes = episodes.into_values().map(|x| x.id).collect::<Vec<_>>();
    let seasons = seasons
        .into_iter()
        .filter(|(number, _)| !synced_seasons.contains(number))
        .map(|(_, id)| id)
        .collect::<Vec<_>>();
    let removed_media_files = old_media_files
        .difference(&media_file_ids)
        .copied()
        .collect::<Vec<_>>();
    changed |= !episodes.is_empty() || !seasons.is_empty() || !removed_media_files.is_empty();
    if !episodes.is_empty() {
        scope
            .episodes()
            .filter(Episode::fields().id().in_list(episodes))
            .delete()
            .exec(scope.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    if !seasons.is_empty() {
        scope
            .seasons()
            .filter(Season::fields().id().in_list(seasons))
            .delete()
            .exec(scope.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    media_files::delete(scope, removed_media_files)
        .await
        .map_err(Error::caused_by)?;
    changed |= metadata::save_artwork(scope, owner, artwork)
        .await
        .map_err(Error::caused_by)?;
    let unreadable_nfos = nfo
        .unreadable_path()
        .map(|path| (None, path.to_owned()))
        .into_iter()
        .chain(
            unreadable_nfos
                .into_iter()
                .map(|(id, path)| (Some(id), path)),
        )
        .collect();
    metadata::save_unreadable_nfos(
        scope,
        owner,
        media_file_ids.into_iter().collect(),
        unreadable_nfos,
    )
    .await
    .map_err(Error::caused_by)?;
    metadata::save_unreadable_file(scope, owner, unreadable)
        .await
        .map_err(Error::caused_by)?;
    Ok(show_change.or(changed.then_some(Change::Changed)))
}
