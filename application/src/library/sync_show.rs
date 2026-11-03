mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::Prober;
use crate::library::library_items;
use crate::library::library_items::Change;
use crate::library::media_files;
use crate::library::media_files::Inspected;
use crate::library::metadata;
use crate::library::metadata::FoundArtwork;
use crate::library::metadata::Nfo;
use crate::library::metadata::Owner;
use crate::ports::EpisodeFolder;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ShowFolder;
use crate::ports::SonarrShow;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::ArtworkKind;
use domain::Episode;
use domain::EpisodeMetadata;
use domain::MediaFile;
use domain::Season;
use domain::Show;
use domain::ShowMetadata;
use domain::Source;
use futures_util::future::try_join_all;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::Instrument;
use tracing::Span;

pub async fn call(dependencies: impl Into<Dependencies>, sonarr_id: i64) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        find_media_file,
        get_sonarr_show,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
        source_health,
    } = dependencies.into();
    let files = ShowFiles {
        find_media_file,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
    };
    async {
        let sonarr_show = get_sonarr_show.call(sonarr_id).await;
        match source_health
            .api_call(Source::Sonarr, sonarr_show)
            .map_err(Error::caused_by)?
        {
            Some(sonarr_show) => save(&mut db_context, &files, sonarr_show).await,
            None => match db_context
                .shows()
                .filter(Show::fields().sonarr_id().eq(sonarr_id))
                .first()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
            {
                Some(show) => remove(&mut db_context, show).await,
                None => Ok(()),
            },
        }
    }
    .instrument(span())
    .await?;
    Ok(Output)
}

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

pub(crate) async fn save(
    db_context: &mut DbContext,
    files: &ShowFiles,
    sonarr_show: SonarrShow,
) -> Result<(), Error> {
    let mut episode_folders = HashMap::new();
    for file in sonarr_show
        .episodes
        .iter()
        .filter_map(|x| x.episode_file.as_ref())
    {
        if !episode_folders.contains_key(&file.path) {
            episode_folders.insert(
                file.path.clone(),
                files.read_episode_folder.call(&file.path).await,
            );
        }
    }
    let season_numbers = sonarr_show
        .episodes
        .iter()
        .map(|x| x.season_number)
        .collect::<BTreeSet<_>>();
    let folder = files
        .read_show_folder
        .call(
            &sonarr_show.listing.path,
            season_numbers.into_iter().collect(),
        )
        .await;
    let stored = if episode_folders.is_empty() {
        HashMap::new()
    } else {
        db_context
            .media_files()
            .filter(
                MediaFile::fields()
                    .path()
                    .in_list(episode_folders.keys().cloned().collect::<Vec<_>>()),
            )
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.path.clone(), x))
            .collect::<HashMap<_, _>>()
    };
    let inspected = try_join_all(episode_folders.keys().map(async |path| {
        let inspected = media_files::inspect(
            &files.find_media_file,
            &files.list_subtitle_files,
            &files.prober,
            stored.get(path),
            path,
        )
        .await?;
        Ok::<_, media_files::Error>((path.clone(), inspected))
    }))
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .collect::<HashMap<_, _>>();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let change = write(
        &mut transaction,
        sonarr_show,
        &episode_folders,
        inspected,
        folder,
        files,
    )
    .await?;
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

async fn write(
    scope: &mut impl DbScope,
    sonarr_show: SonarrShow,
    episode_folders: &HashMap<String, EpisodeFolder>,
    mut inspected: HashMap<String, Inspected>,
    folder: ShowFolder,
    files: &ShowFiles,
) -> Result<Option<Change>, Error> {
    let listing = sonarr_show.listing;
    let stored = scope
        .shows()
        .filter(Show::fields().tvdb_id().eq(listing.tvdb_id))
        .first()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let nfo = Nfo::read(
        folder.nfo,
        stored.as_ref().and_then(|x| x.nfo_modified_at),
        |path| files.read_show_nfo.call(path),
    )
    .await;
    let had_nfo = stored.as_ref().is_some_and(|x| x.nfo_modified_at.is_some());
    let sonarr_metadata =
        ShowMetadata::from_sonarr(listing.title, listing.overview, listing.genres);
    let (metadata, nfo_modified_at) = match (&nfo, &stored) {
        (Nfo::Unchanged, Some(show)) => (show.metadata(), show.nfo_modified_at),
        (Nfo::Read(nfo, modified_at), _) => (sonarr_metadata.with_nfo(nfo), Some(*modified_at)),
        _ => (sonarr_metadata, None),
    };
    let (show, show_change) = match stored {
        Some(mut show) => {
            let changed = show.sonarr_id != listing.sonarr_id
                || show.year != listing.year
                || show.nfo_modified_at != nfo_modified_at
                || show.metadata() != metadata;
            if changed {
                show.update()
                    .sonarr_id(listing.sonarr_id)
                    .year(listing.year)
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
                    .exec(scope.executor_mut())
                    .await
                    .map_err(Error::caused_by)?;
            }
            (show, changed.then_some(Change::Changed))
        }
        None => (
            scope
                .shows()
                .create()
                .tvdb_id(listing.tvdb_id)
                .sonarr_id(listing.sonarr_id)
                .year(listing.year)
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
    let mut nfo_roots = HashMap::new();
    let mut unreadable_nfos = HashMap::new();
    let mut saved = HashMap::new();
    let sonarr_ids = sonarr_show
        .episodes
        .iter()
        .map(|x| x.sonarr_id)
        .collect::<HashSet<_>>();
    for sonarr_episode in sonarr_show.episodes {
        let episode_folder = sonarr_episode
            .episode_file
            .as_ref()
            .and_then(|x| episode_folders.get(&x.path));
        let stored = episodes.remove(&sonarr_episode.tvdb_id);
        let nfo = match episode_folder.and_then(|x| x.nfo.as_ref()) {
            None => Nfo::Missing,
            Some(nfo_file)
                if stored
                    .as_ref()
                    .is_some_and(|x| x.nfo_modified_at == Some(nfo_file.modified_at)) =>
            {
                Nfo::Unchanged
            }
            Some(nfo_file) => {
                if !nfo_roots.contains_key(&nfo_file.path) {
                    let roots = files.read_episode_nfo.call(&nfo_file.path).await;
                    if let Err(error) = &roots {
                        metadata::warn_unreadable(error, &nfo_file.path);
                    }
                    nfo_roots.insert(nfo_file.path.clone(), roots.ok());
                }
                match &nfo_roots[&nfo_file.path] {
                    None => Nfo::Unreadable(nfo_file.path.clone()),
                    Some(roots) => match roots
                        .iter()
                        .find(|x| x.sonarr_id == Some(sonarr_episode.sonarr_id))
                        .or_else(|| {
                            roots.iter().find(|x| {
                                !x.sonarr_id.is_some_and(|x| sonarr_ids.contains(&x))
                                    && x.season_number == Some(sonarr_episode.season_number)
                                    && x.episode_number == Some(sonarr_episode.episode_number)
                            })
                        }) {
                        Some(root) => Nfo::Read(root.clone(), nfo_file.modified_at),
                        None => Nfo::Missing,
                    },
                }
            }
        };
        let sonarr_metadata = EpisodeMetadata::from_sonarr(
            sonarr_episode.title,
            sonarr_episode.air_date,
            sonarr_episode.overview,
        );
        let (metadata, nfo_modified_at) = match (&nfo, &stored) {
            (Nfo::Unchanged, Some(episode)) => (episode.metadata(), episode.nfo_modified_at),
            (Nfo::Read(nfo, modified_at), _) => (sonarr_metadata.with_nfo(nfo), Some(*modified_at)),
            _ => (sonarr_metadata, None),
        };
        let season_number = sonarr_episode.season_number;
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
        let (media_file_id, file_on_disk) = match &sonarr_episode.episode_file {
            Some(sonarr_file) => {
                let (id, on_disk) = match inspected.remove(&sonarr_file.path) {
                    Some(inspected) => {
                        let on_disk = inspected.on_disk();
                        let (id, file_changed) =
                            media_files::save(scope, &sonarr_file.path, inspected)
                                .await
                                .map_err(Error::caused_by)?;
                        changed |= file_changed;
                        saved.insert(sonarr_file.path.clone(), (id, on_disk));
                        (id, on_disk)
                    }
                    None => saved[&sonarr_file.path],
                };
                media_file_ids.insert(id);
                if let Nfo::Unreadable(path) = &nfo {
                    unreadable_nfos.insert(id, path.clone());
                }
                (Some(id), on_disk)
            }
            None => (None, false),
        };
        let visible = Episode::in_library(sonarr_episode.episode_file.is_some(), file_on_disk);
        let date_added = sonarr_episode.episode_file.map(|x| x.date_added);
        let episode_id = match stored {
            Some(mut episode) => {
                let added_at = domain::first_added_at(episode.added_at, date_added);
                if episode.season_id != season_id
                    || episode.number != sonarr_episode.episode_number
                    || episode.nfo_modified_at != nfo_modified_at
                    || episode.metadata() != metadata
                    || episode.added_at != added_at
                    || episode.media_file_id != media_file_id
                    || episode.visible != visible
                {
                    changed = true;
                    episode
                        .update()
                        .season_id(season_id)
                        .number(sonarr_episode.episode_number)
                        .title(metadata.title)
                        .air_date(metadata.air_date)
                        .overview(metadata.overview)
                        .rating(metadata.rating)
                        .nfo_modified_at(nfo_modified_at)
                        .added_at(added_at)
                        .media_file_id(media_file_id)
                        .visible(visible)
                        .exec(scope.executor_mut())
                        .await
                        .map_err(Error::caused_by)?;
                }
                episode.id
            }
            None => {
                changed = true;
                scope
                    .episodes()
                    .create()
                    .season_id(season_id)
                    .tvdb_id(sonarr_episode.tvdb_id)
                    .number(sonarr_episode.episode_number)
                    .title(metadata.title)
                    .air_date(metadata.air_date)
                    .overview(metadata.overview)
                    .rating(metadata.rating)
                    .nfo_modified_at(nfo_modified_at)
                    .added_at(date_added)
                    .media_file_id(media_file_id)
                    .visible(visible)
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
    Ok(show_change.or(changed.then_some(Change::Changed)))
}
