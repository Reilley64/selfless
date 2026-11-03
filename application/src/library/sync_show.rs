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
use crate::ports::ReadNfoError;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ShowFolder;
use crate::ports::SonarrEpisode;
use crate::ports::SonarrShow;
use crate::ports::UnreadableFile;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::ArtworkKind;
use domain::Episode;
use domain::EpisodeMetadata;
use domain::EpisodeNfo;
use domain::MediaFile;
use domain::Season;
use domain::Show;
use domain::ShowMetadata;
use domain::ShowNfo;
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
            Some(sonarr_show) => {
                let inspected = inspect(&mut db_context, &files, &sonarr_show).await?;
                save(&mut db_context, sonarr_show, inspected).await
            }
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
    sonarr_show: &SonarrShow,
) -> Result<InspectedShow, Error> {
    let episode_file_paths = sonarr_show
        .episodes
        .iter()
        .filter_map(|x| x.episode_file.as_ref())
        .map(|x| x.path.clone())
        .collect::<Vec<_>>();
    let stored = if episode_file_paths.is_empty() {
        HashMap::new()
    } else {
        db_context
            .media_files()
            .filter(MediaFile::fields().path().in_list(episode_file_paths))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.path.clone(), x))
            .collect::<HashMap<_, _>>()
    };
    let stored_show = db_context
        .shows()
        .filter(Show::fields().tvdb_id().eq(sonarr_show.listing.tvdb_id))
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
    let sonarr_ids = sonarr_show
        .episodes
        .iter()
        .map(|x| x.sonarr_id)
        .collect::<HashSet<_>>();
    let read = async {
        let mut found = HashMap::new();
        let mut episode_folders = HashMap::new();
        for file in sonarr_show
            .episodes
            .iter()
            .filter_map(|x| x.episode_file.as_ref())
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
        let season_numbers = sonarr_show
            .episodes
            .iter()
            .map(|x| x.season_number)
            .collect::<BTreeSet<_>>();
        let mut folder = files
            .read_show_folder
            .call(
                &sonarr_show.listing.path,
                season_numbers.into_iter().collect(),
            )
            .await?;
        let nfo = Nfo::read(
            folder.nfo.take(),
            stored_show.as_ref().and_then(|x| x.nfo_modified_at),
            |path| files.read_show_nfo.call(path),
        )
        .await?;
        let mut nfo_roots = HashMap::new();
        let mut episode_nfos = Vec::new();
        for sonarr_episode in &sonarr_show.episodes {
            let nfo_file = sonarr_episode
                .episode_file
                .as_ref()
                .and_then(|x| episode_folders.get(&x.path))
                .and_then(|x| x.nfo.as_ref());
            episode_nfos.push(match nfo_file {
                None => Nfo::Missing,
                Some(nfo_file)
                    if stored_nfo_modified_at.get(&sonarr_episode.tvdb_id)
                        == Some(&Some(nfo_file.modified_at)) =>
                {
                    Nfo::Unchanged
                }
                Some(nfo_file) => {
                    if !nfo_roots.contains_key(&nfo_file.path) {
                        let roots = match files.read_episode_nfo.call(&nfo_file.path).await {
                            Ok(roots) => Some(roots),
                            Err(ReadNfoError::UnreadableNfo(error)) => {
                                metadata::warn_unreadable(&error, &nfo_file.path);
                                None
                            }
                            Err(ReadNfoError::UnreadableFile(error)) => return Err(error),
                        };
                        nfo_roots.insert(nfo_file.path.clone(), roots);
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
            });
        }
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
    sonarr_show: SonarrShow,
    inspected: InspectedShow,
) -> Result<(), Error> {
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let change = match inspected {
        InspectedShow::Read(on_disk) => {
            write(&mut transaction, sonarr_show, *on_disk, None).await?
        }
        InspectedShow::Unreadable(error) => match transaction
            .shows()
            .filter(Show::fields().tvdb_id().eq(sonarr_show.listing.tvdb_id))
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
                    episode_nfos: sonarr_show.episodes.iter().map(|_| Nfo::Missing).collect(),
                };
                let sonarr_show = SonarrShow {
                    episodes: sonarr_show
                        .episodes
                        .into_iter()
                        .map(|x| SonarrEpisode {
                            episode_file: None,
                            ..x
                        })
                        .collect(),
                    ..sonarr_show
                };
                write(&mut transaction, sonarr_show, on_disk, Some(error.path)).await?
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

async fn write(
    scope: &mut impl DbScope,
    sonarr_show: SonarrShow,
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
    let listing = sonarr_show.listing;
    let stored = scope
        .shows()
        .filter(Show::fields().tvdb_id().eq(listing.tvdb_id))
        .first()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?;
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
    let mut unreadable_nfos = HashMap::new();
    let mut saved = HashMap::new();
    for (sonarr_episode, nfo) in sonarr_show.episodes.into_iter().zip(episode_nfos) {
        let episode_folder = sonarr_episode
            .episode_file
            .as_ref()
            .and_then(|x| episode_folders.get(&x.path));
        let stored = episodes.remove(&sonarr_episode.tvdb_id);
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
                        let saved_file = media_files::save(scope, &sonarr_file.path, inspected)
                            .await
                            .map_err(Error::caused_by)?;
                        changed |= saved_file.changed;
                        saved.insert(sonarr_file.path.clone(), (saved_file.id, on_disk));
                        (saved_file.id, on_disk)
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
    metadata::save_unreadable_file(scope, owner, unreadable)
        .await
        .map_err(Error::caused_by)?;
    Ok(show_change.or(changed.then_some(Change::Changed)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::FoundFile;
    use crate::ports::SonarrEpisode;
    use crate::test::S01E01_FILE;
    use crate::test::S01E02_E03_FILE;
    use crate::test::S02E01_FILE;
    use crate::test::SHOW_FOLDER;
    use crate::test::SPECIAL_FILE;
    use crate::test::ShowLibrary;
    use crate::test::at;
    use crate::test::severance;
    use crate::test::sonarr_episode_file;
    use domain::Actor;
    use domain::Artwork;
    use domain::EpisodeNfo;
    use domain::NfoActor;
    use domain::ShowNfo;
    use domain::SyncProblem;
    use domain::SyncProblemKind;
    use jiff::civil::date;
    use uuid::Uuid;

    const UPGRADED_S02E01_FILE: &str =
        "/media/TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel Bluray-2160p.mkv";
    const SHOW_NFO: &str = "/media/TV/Severance/tvshow.nfo";

    async fn show(library: &ShowLibrary) -> Show {
        let [show] = shows(library).await.try_into().unwrap();
        show
    }

    async fn shows(library: &ShowLibrary) -> Vec<Show> {
        let mut db_context = library.db_context.clone();
        db_context
            .shows()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn season_numbers(library: &ShowLibrary) -> HashMap<Uuid, i64> {
        let mut db_context = library.db_context.clone();
        db_context
            .seasons()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.id, x.number))
            .collect()
    }

    async fn episodes(library: &ShowLibrary) -> Vec<(i64, Episode)> {
        let mut db_context = library.db_context.clone();
        let season_numbers = season_numbers(library).await;
        let mut episodes = db_context
            .episodes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (season_numbers[&x.season_id], x))
            .collect::<Vec<_>>();
        episodes.sort_by_key(|(season, x)| (*season, x.number));
        episodes
    }

    async fn episode(library: &ShowLibrary, season: i64, number: i64) -> Episode {
        episodes(library)
            .await
            .into_iter()
            .find(|(s, x)| *s == season && x.number == number)
            .unwrap()
            .1
    }

    async fn titles_and_ratings(
        library: &ShowLibrary,
        season: i64,
    ) -> Vec<(i64, String, Option<f64>)> {
        episodes(library)
            .await
            .into_iter()
            .filter(|(s, _)| *s == season)
            .map(|(_, x)| (x.number, x.title, x.rating))
            .collect()
    }

    async fn actors(library: &ShowLibrary) -> Vec<(String, Option<String>, Option<i64>)> {
        let mut db_context = library.db_context.clone();
        let mut actors = db_context
            .actors()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x: Actor| (x.name, x.role, x.order))
            .collect::<Vec<_>>();
        actors.sort_by_key(|x| x.2);
        actors
    }

    async fn artwork(library: &ShowLibrary) -> Vec<Artwork> {
        let mut db_context = library.db_context.clone();
        let mut artwork = db_context
            .artwork()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        artwork.sort_by(|a, b| (&a.path, a.episode_id).cmp(&(&b.path, b.episode_id)));
        artwork
    }

    async fn problems(library: &ShowLibrary) -> Vec<SyncProblem> {
        let mut db_context = library.db_context.clone();
        db_context
            .sync_problems()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn media_files(library: &ShowLibrary) -> Vec<MediaFile> {
        let mut db_context = library.db_context.clone();
        db_context
            .media_files()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    fn found(path: &str, modified_at: &str) -> Option<FoundFile> {
        Some(FoundFile {
            path: path.to_owned(),
            modified_at: at(modified_at),
        })
    }

    fn nfo_path(media_file: &str) -> String {
        media_file.replace(".mkv", ".nfo")
    }

    fn show_nfo() -> ShowNfo {
        ShowNfo {
            title: Some("Severance".to_owned()),
            sort_title: Some("severance".to_owned()),
            plot: Some("Mark leads a team whose memories are surgically divided.".to_owned()),
            certification: Some("TV-MA".to_owned()),
            premiered: Some(date(2022, 2, 18)),
            end_date: None,
            status: Some("Continuing".to_owned()),
            network: Some("Apple TV+".to_owned()),
            genres: vec!["Drama".to_owned(), "Thriller".to_owned()],
            rating: Some(8.7),
            actors: vec![
                nfo_actor("Adam Scott", "Mark Scout", 0),
                nfo_actor("Britt Lower", "Helly R.", 1),
                nfo_actor("Zach Cherry", "Dylan George", 2),
            ],
        }
    }

    fn nfo_actor(name: &str, role: &str, order: i64) -> NfoActor {
        NfoActor {
            name: name.to_owned(),
            role: Some(role.to_owned()),
            order: Some(order),
        }
    }

    fn episode_nfo(
        sonarr_id: Option<i64>,
        season_number: i64,
        episode_number: i64,
        title: &str,
        rating: f64,
    ) -> EpisodeNfo {
        EpisodeNfo {
            sonarr_id,
            title: Some(title.to_owned()),
            season_number: Some(season_number),
            episode_number: Some(episode_number),
            air_date: None,
            plot: Some(format!("The NFO's plot of {title}")),
            rating: Some(rating),
        }
    }

    fn with_show_nfo(library: &ShowLibrary, nfo: Option<ShowNfo>, modified_at: &str) {
        let mut sources = library.sources();
        sources.folder.nfo = found(SHOW_NFO, modified_at);
        sources.nfo = nfo;
    }

    fn with_episode_nfo(
        library: &ShowLibrary,
        media_file: &str,
        roots: Option<Vec<EpisodeNfo>>,
        modified_at: &str,
    ) {
        let mut sources = library.sources();
        let path = nfo_path(media_file);
        sources
            .episode_folders
            .entry(media_file.to_owned())
            .or_default()
            .nfo = found(&path, modified_at);
        match roots {
            Some(roots) => sources.episode_nfos.insert(path, roots),
            None => sources.episode_nfos.remove(&path),
        };
    }

    fn with_sonarr_episodes(library: &ShowLibrary, change: impl FnOnce(&mut Vec<SonarrEpisode>)) {
        let mut sources = library.sources();
        change(&mut sources.sonarr_show.as_mut().unwrap().episodes);
    }

    fn severance_episodes(library: &ShowLibrary) -> Vec<SonarrEpisode> {
        library
            .sources()
            .sonarr_show
            .as_ref()
            .unwrap()
            .episodes
            .clone()
    }

    fn sonarr_episode(
        episodes: &mut [SonarrEpisode],
        season: i64,
        number: i64,
    ) -> &mut SonarrEpisode {
        episodes
            .iter_mut()
            .find(|x| x.season_number == season && x.episode_number == number)
            .unwrap()
    }

    async fn unreadable_file_problems(library: &ShowLibrary) -> Vec<(Option<Uuid>, String)> {
        problems(library)
            .await
            .into_iter()
            .filter(|x| x.kind == SyncProblemKind::UnreadableFile)
            .map(|x| (x.show_id, x.path))
            .collect()
    }

    fn retitle_show(library: &ShowLibrary) {
        library
            .sources()
            .sonarr_show
            .as_mut()
            .unwrap()
            .listing
            .title = "Severance (2022)".to_owned();
    }

    #[tokio::test]
    async fn an_unreadable_episode_file_keeps_the_stored_show_and_records_an_unreadable_file_until_it_is_read()
     {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let before = show(&library).await;

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(S01E01_FILE.to_owned());
            sources.files_on_disk.remove(S02E01_FILE);
        }
        retitle_show(&library);
        library.sync_show().await.unwrap();
        let kept = show(&library).await;
        let kept_visible = episode(&library, 2, 1).await.visible;
        let recorded = unreadable_file_problems(&library).await;
        library.sources().unreadable_files.clear();
        library.sync_show().await.unwrap();

        assert_eq!(kept.id, before.id);
        assert_eq!(kept.title, "Severance");
        assert!(kept_visible);
        assert_eq!(recorded, [(Some(before.id), S01E01_FILE.to_owned())]);
        assert_eq!(show(&library).await.title, "Severance (2022)");
        assert!(!episode(&library, 2, 1).await.visible);
        assert!(unreadable_file_problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_file_beside_an_episode_keeps_the_stored_artwork() {
        let library = ShowLibrary::new().await;
        let thumb = S01E01_FILE.replace(".mkv", "-thumb.jpg");
        library
            .sources()
            .episode_folders
            .entry(S01E01_FILE.to_owned())
            .or_default()
            .thumb = found(&thumb, "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(thumb.clone());
            sources.episode_folders.get_mut(S01E01_FILE).unwrap().thumb =
                found(&thumb, "2026-10-30T08:00:00Z");
        }
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [(thumb.clone(), at("2026-10-29T11:00:00Z"))]
        );
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), thumb)]
        );
    }

    #[tokio::test]
    async fn an_unreadable_file_in_the_show_folder_keeps_the_stored_artwork_and_metadata() {
        let library = ShowLibrary::new().await;
        let poster = format!("{SHOW_FOLDER}/poster.jpg");
        library.sources().folder.poster = found(&poster, "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        {
            let mut sources = library.sources();
            sources.unreadable_files.insert(poster.clone());
            sources.folder.poster = found(&poster, "2026-10-30T08:00:00Z");
        }
        retitle_show(&library);
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [(poster.clone(), at("2026-10-29T11:00:00Z"))]
        );
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), poster)]
        );
    }

    #[tokio::test]
    async fn a_show_nfo_that_cannot_be_read_keeps_the_stored_nfo_metadata_and_records_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        let retitled = ShowNfo {
            title: Some("Severance (NFO)".to_owned()),
            ..show_nfo()
        };
        with_show_nfo(&library, Some(retitled), "2026-10-30T08:00:00Z");
        library
            .sources()
            .unreadable_files
            .insert(SHOW_NFO.to_owned());
        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(show.sort_title.as_deref(), Some("severance"));
        assert_eq!(show.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.show_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::UnreadableFile,
                Some(show.id),
                SHOW_NFO.to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_that_cannot_be_read_keeps_the_stored_nfo_metadata_and_records_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.1,
            )]),
            "2026-10-29T11:00:00Z",
        );
        library.sync_show().await.unwrap();

        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                9.0,
            )]),
            "2026-10-30T08:00:00Z",
        );
        library
            .sources()
            .unreadable_files
            .insert(nfo_path(S01E01_FILE));
        library.sync_show().await.unwrap();

        let episode = episode(&library, 1, 1).await;
        assert_eq!(episode.rating, Some(8.1));
        assert_eq!(episode.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.show_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::UnreadableFile,
                Some(show(&library).await.id),
                nfo_path(S01E01_FILE)
            )]
        );
    }

    #[tokio::test]
    async fn a_new_show_with_an_unreadable_file_is_stored_without_visible_episodes_and_with_an_unreadable_file()
     {
        let library = ShowLibrary::new().await;
        library
            .sources()
            .unreadable_files
            .insert(S01E01_FILE.to_owned());

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert!(episodes(&library).await.iter().all(|(_, x)| !x.visible));
        assert!(media_files(&library).await.is_empty());
        assert_eq!(
            unreadable_file_problems(&library).await,
            [(Some(show.id), S01E01_FILE.to_owned())]
        );
    }

    #[tokio::test]
    async fn a_sonarr_show_is_stored_with_sonarrs_fields_and_only_episodes_with_a_file_are_visible()
    {
        let library = ShowLibrary::new().await;

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.sonarr_id, 7);
        assert_eq!(show.tvdb_id, 371_980);
        assert_eq!(show.title, "Severance");
        assert_eq!(show.year, Some(2022));
        assert_eq!(show.genres, ["Drama", "Mystery", "Science Fiction"]);
        assert_eq!(
            show.overview.as_deref(),
            Some("Mark leads a team of office workers.")
        );
        let mut seasons = season_numbers(&library)
            .await
            .into_values()
            .collect::<Vec<_>>();
        seasons.sort_unstable();
        assert_eq!(seasons, [0, 1, 2]);
        let episode = episode(&library, 2, 1).await;
        assert_eq!(episode.tvdb_id, 10_535_541);
        assert_eq!(episode.title, "Hello, Ms. Cobel");
        assert_eq!(episode.air_date, Some(date(2025, 1, 17)));
        assert_eq!(
            episode.overview.as_deref(),
            Some("Mark leads his team on a search for answers.")
        );
        assert_eq!(episode.added_at, Some(at("2025-01-17T03:12:44Z")));
        assert_eq!(
            episodes(&library)
                .await
                .into_iter()
                .map(|(season, x)| (season, x.number, x.visible))
                .collect::<Vec<_>>(),
            [
                (0, 1, true),
                (1, 1, true),
                (1, 2, true),
                (1, 3, true),
                (2, 1, true),
                (2, 2, false),
            ]
        );
    }

    #[tokio::test]
    async fn a_file_holding_two_episodes_gives_each_episode_its_own_row_pointing_at_the_one_file() {
        let library = ShowLibrary::new().await;

        library.sync_show().await.unwrap();

        let first = episode(&library, 1, 1).await;
        let second = episode(&library, 1, 2).await;
        let third = episode(&library, 1, 3).await;
        assert_ne!(second.id, third.id);
        assert!(second.media_file_id.is_some());
        assert_eq!(second.media_file_id, third.media_file_id);
        assert_ne!(first.media_file_id, second.media_file_id);
        assert_eq!(
            media_files(&library)
                .await
                .into_iter()
                .filter(|x| x.path == S01E02_E03_FILE)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn an_episode_whose_file_is_missing_on_disk_is_hidden() {
        let library = ShowLibrary::new().await;
        library.sources().files_on_disk.remove(S01E01_FILE);

        library.sync_show().await.unwrap();

        assert!(!episode(&library, 1, 1).await.visible);
        assert!(episode(&library, 1, 2).await.visible);
    }

    #[tokio::test]
    async fn a_shared_file_missing_on_disk_records_one_file_not_found_problem() {
        let library = ShowLibrary::new().await;
        library.sources().files_on_disk.remove(S01E02_E03_FILE);

        library.sync_show().await.unwrap();

        let media_file_id = episode(&library, 1, 2).await.media_file_id;
        assert_eq!(
            problems(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.media_file_id, x.path))
                .collect::<Vec<_>>(),
            [(
                SyncProblemKind::FileNotFound,
                media_file_id,
                S01E02_E03_FILE.to_owned()
            )]
        );
    }

    #[tokio::test]
    async fn a_show_without_files_is_hidden_and_files_again_show_it_with_the_same_rows() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let show_before = show(&library).await;
        let episodes_before = episodes(&library)
            .await
            .into_iter()
            .map(|(_, x)| x.id)
            .collect::<Vec<_>>();

        let with_files = severance_episodes(&library);
        with_sonarr_episodes(&library, |episodes| {
            for episode in episodes {
                episode.episode_file = None;
            }
        });
        library.sync_show().await.unwrap();
        let hidden = episodes(&library).await;
        with_sonarr_episodes(&library, |episodes| *episodes = with_files);
        library.sync_show().await.unwrap();

        assert!(hidden.iter().all(|(_, x)| !x.visible));
        assert_eq!(show(&library).await.id, show_before.id);
        let episodes_after = episodes(&library).await;
        assert_eq!(
            episodes_after.iter().map(|(_, x)| x.id).collect::<Vec<_>>(),
            episodes_before
        );
        assert!(episode(&library, 1, 1).await.visible);
    }

    #[tokio::test]
    async fn an_upgraded_file_keeps_the_episode_and_its_added_date() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let before = episode(&library, 2, 1).await;

        library
            .sources()
            .files_on_disk
            .insert(UPGRADED_S02E01_FILE.to_owned());
        with_sonarr_episodes(&library, |episodes| {
            sonarr_episode(episodes, 2, 1).episode_file =
                sonarr_episode_file(UPGRADED_S02E01_FILE, "2026-10-31T10:00:00Z");
        });
        library.sync_show().await.unwrap();

        let after = episode(&library, 2, 1).await;
        assert_eq!(after.id, before.id);
        assert_eq!(after.added_at, Some(at("2025-01-17T03:12:44Z")));
        assert_ne!(after.media_file_id, before.media_file_id);
        assert!(after.visible);
    }

    #[tokio::test]
    async fn a_show_sonarr_no_longer_has_is_removed_with_its_seasons_episodes_actors_artwork_problems_and_media_files()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        with_episode_nfo(&library, S01E01_FILE, None, "2026-10-29T11:00:00Z");
        library.sources().folder.poster =
            found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-29T11:00:00Z");
        library.sources().files_on_disk.remove(S02E01_FILE);
        library.sync_show().await.unwrap();
        let before = show(&library).await;

        library.sources().sonarr_show = None;
        library.sync_show().await.unwrap();

        assert!(shows(&library).await.is_empty());
        assert!(season_numbers(&library).await.is_empty());
        assert!(episodes(&library).await.is_empty());
        assert!(actors(&library).await.is_empty());
        assert!(artwork(&library).await.is_empty());
        assert!(problems(&library).await.is_empty());
        assert!(media_files(&library).await.is_empty());
        library.sources().sonarr_show = Some(severance());
        library.sync_show().await.unwrap();
        assert_ne!(show(&library).await.id, before.id);
    }

    #[tokio::test]
    async fn a_show_stores_the_fields_and_actors_of_its_tvshow_nfo() {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.title, "Severance");
        assert_eq!(show.sort_title.as_deref(), Some("severance"));
        assert_eq!(
            show.overview.as_deref(),
            Some("Mark leads a team whose memories are surgically divided.")
        );
        assert_eq!(show.certification.as_deref(), Some("TV-MA"));
        assert_eq!(show.premiered, Some(date(2022, 2, 18)));
        assert_eq!(show.end_date, None);
        assert_eq!(show.status.as_deref(), Some("Continuing"));
        assert_eq!(show.network.as_deref(), Some("Apple TV+"));
        assert_eq!(show.genres, ["Drama", "Thriller"]);
        assert_eq!(show.rating, Some(8.7));
        assert_eq!(show.nfo_modified_at, Some(at("2026-10-29T11:00:00Z")));
        assert_eq!(
            actors(&library).await,
            [
                (
                    "Adam Scott".to_owned(),
                    Some("Mark Scout".to_owned()),
                    Some(0)
                ),
                (
                    "Britt Lower".to_owned(),
                    Some("Helly R.".to_owned()),
                    Some(1)
                ),
                (
                    "Zach Cherry".to_owned(),
                    Some("Dylan George".to_owned()),
                    Some(2)
                ),
            ]
        );
        assert!(problems(&library).await.is_empty());
    }

    #[tokio::test]
    async fn a_new_show_without_a_tvshow_nfo_takes_sonarrs_fields_until_a_later_sync_finds_one() {
        let library = ShowLibrary::new().await;
        library.sync_show().await.unwrap();
        let without_nfo = show(&library).await;
        let actors_without_nfo = actors(&library).await;

        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        library.sync_show().await.unwrap();

        assert_eq!(without_nfo.network, None);
        assert_eq!(without_nfo.sort_title, None);
        assert!(actors_without_nfo.is_empty());
        assert!(problems(&library).await.is_empty());
        assert_eq!(show(&library).await.network.as_deref(), Some("Apple TV+"));
        assert_eq!(actors(&library).await.len(), 3);
    }

    #[tokio::test]
    async fn unreadable_nfos_store_sonarrs_fields_and_one_problem_for_the_show_nfo_and_one_per_media_file()
     {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, None, "2026-10-29T11:00:00Z");
        with_episode_nfo(&library, S01E02_E03_FILE, None, "2026-10-29T11:00:00Z");

        library.sync_show().await.unwrap();

        let show = show(&library).await;
        assert_eq!(show.sort_title, None);
        assert_eq!(show.genres, ["Drama", "Mystery", "Science Fiction"]);
        let half_loop = episode(&library, 1, 2).await;
        assert_eq!(half_loop.title, "Half Loop");
        assert_eq!(
            half_loop.overview.as_deref(),
            Some("Mark takes Helly on a tour of the office.")
        );
        assert_eq!(half_loop.rating, None);
        let mut problems = problems(&library)
            .await
            .into_iter()
            .map(|x| (x.kind, x.show_id, x.media_file_id, x.path))
            .collect::<Vec<_>>();
        problems.sort_by(|a, b| a.3.cmp(&b.3));
        assert_eq!(
            problems,
            [
                (
                    SyncProblemKind::UnreadableNfo,
                    None,
                    half_loop.media_file_id,
                    nfo_path(S01E02_E03_FILE)
                ),
                (
                    SyncProblemKind::UnreadableNfo,
                    Some(show.id),
                    None,
                    SHOW_NFO.to_owned()
                ),
            ]
        );
        assert_eq!(library.sources().episode_nfo_reads, 1);
    }

    #[tokio::test]
    async fn an_nfo_with_two_episodes_fills_each_of_them() {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E02_E03_FILE,
            Some(vec![
                episode_nfo(Some(1012), 1, 2, "Half Loop", 8.1),
                episode_nfo(Some(1013), 1, 3, "In Perpetuity", 8.3),
            ]),
            "2026-10-29T11:00:00Z",
        );

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (1, "Good News About Hell".to_owned(), None),
                (2, "Half Loop".to_owned(), Some(8.1)),
                (3, "In Perpetuity".to_owned(), Some(8.3)),
            ]
        );
        assert_eq!(
            episode(&library, 1, 3).await.overview.as_deref(),
            Some("The NFO's plot of In Perpetuity")
        );
        assert_eq!(
            episode(&library, 1, 3).await.air_date,
            Some(date(2022, 2, 25))
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_matches_by_sonarr_id_then_by_season_and_episode_number_and_keeps_sonarrs_numbers()
     {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S02E01_FILE,
            Some(vec![episode_nfo(Some(1021), 3, 9, "Hello, Ms. Cobel", 8.6)]),
            "2026-10-29T11:00:00Z",
        );
        with_episode_nfo(
            &library,
            SPECIAL_FILE,
            Some(vec![episode_nfo(None, 0, 1, "Inside the Episode", 7.2)]),
            "2026-10-29T11:00:00Z",
        );

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 2).await,
            [
                (1, "Hello, Ms. Cobel".to_owned(), Some(8.6)),
                (2, "Goodbye, Mrs. Selvig".to_owned(), None),
            ]
        );
        assert_eq!(
            titles_and_ratings(&library, 0).await,
            [(1, "Inside the Episode".to_owned(), Some(7.2))]
        );
    }

    #[tokio::test]
    async fn an_unchanged_episode_nfo_takes_sonarrs_new_episode_number() {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.0,
            )]),
            "2026-10-29T11:00:00Z",
        );
        library.sync_show().await.unwrap();

        with_sonarr_episodes(&library, |episodes| {
            sonarr_episode(episodes, 1, 1).episode_number = 4;
        });
        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (2, "Half Loop".to_owned(), None),
                (3, "In Perpetuity".to_owned(), None),
                (4, "Good News About Hell".to_owned(), Some(8.0)),
            ]
        );
        assert_eq!(library.sources().episode_nfo_reads, 1);
    }

    #[tokio::test]
    async fn an_episode_nfo_of_a_re_added_show_matches_by_season_and_episode_number() {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E02_E03_FILE,
            Some(vec![
                episode_nfo(Some(1012), 1, 2, "Half Loop", 8.1),
                episode_nfo(Some(1013), 1, 3, "In Perpetuity", 8.3),
            ]),
            "2026-10-29T11:00:00Z",
        );
        with_sonarr_episodes(&library, |episodes| {
            for episode in episodes {
                episode.sonarr_id += 10_000;
            }
        });

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (1, "Good News About Hell".to_owned(), None),
                (2, "Half Loop".to_owned(), Some(8.1)),
                (3, "In Perpetuity".to_owned(), Some(8.3)),
            ]
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_entry_whose_sonarr_id_belongs_to_another_episode_is_not_matched_by_number()
     {
        let library = ShowLibrary::new().await;
        with_episode_nfo(
            &library,
            S01E02_E03_FILE,
            Some(vec![
                episode_nfo(Some(1012), 1, 2, "Half Loop", 8.1),
                episode_nfo(Some(1013), 1, 3, "In Perpetuity", 8.3),
            ]),
            "2026-10-29T11:00:00Z",
        );
        with_sonarr_episodes(&library, |episodes| {
            sonarr_episode(episodes, 1, 2).episode_number = 4;
            let in_perpetuity = sonarr_episode(episodes, 1, 3);
            in_perpetuity.episode_number = 2;
            in_perpetuity.sonarr_id = 1014;
        });

        library.sync_show().await.unwrap();

        assert_eq!(
            titles_and_ratings(&library, 1).await,
            [
                (1, "Good News About Hell".to_owned(), None),
                (2, "In Perpetuity".to_owned(), None),
                (4, "Half Loop".to_owned(), Some(8.1)),
            ]
        );
    }

    #[tokio::test]
    async fn show_artwork_season_posters_for_synced_seasons_and_episode_thumbs_are_stored_with_their_owners()
     {
        let library = ShowLibrary::new().await;
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart =
                found(&format!("{SHOW_FOLDER}/fanart.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.clearlogo = found(
                &format!("{SHOW_FOLDER}/clearlogo.png"),
                "2026-10-28T12:30:00Z",
            );
            sources.folder.season_posters = [
                (0, "season-specials-poster.jpg"),
                (1, "season01-poster.jpg"),
                (4, "season04-poster.jpg"),
            ]
            .into_iter()
            .map(|(number, name)| {
                (
                    number,
                    found(&format!("{SHOW_FOLDER}/{name}"), "2026-10-28T12:31:00Z").unwrap(),
                )
            })
            .collect();
            for media_file in [S01E01_FILE, S01E02_E03_FILE] {
                sources
                    .episode_folders
                    .entry(media_file.to_owned())
                    .or_default()
                    .thumb = found(
                    &media_file.replace(".mkv", "-thumb.jpg"),
                    "2026-10-28T12:32:00Z",
                );
            }
        }

        library.sync_show().await.unwrap();

        let show_id = show(&library).await.id;
        let season_numbers = season_numbers(&library).await;
        let episode_numbers = episodes(&library)
            .await
            .into_iter()
            .map(|(season, x)| (x.id, (season, x.number)))
            .collect::<HashMap<_, _>>();
        let artwork = artwork(&library)
            .await
            .into_iter()
            .map(|x| {
                assert_eq!(x.show_id, Some(show_id));
                (
                    x.kind,
                    x.path.strip_prefix(SHOW_FOLDER).unwrap().to_owned(),
                    x.season_id.map(|id| season_numbers[&id]),
                    x.episode_id.map(|id| episode_numbers[&id]),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            artwork,
            [
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 1))
                ),
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 2))
                ),
                (
                    ArtworkKind::Thumb,
                    "/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p-thumb.jpg"
                        .to_owned(),
                    None,
                    Some((1, 3))
                ),
                (ArtworkKind::Clearlogo, "/clearlogo.png".to_owned(), None, None),
                (ArtworkKind::Fanart, "/fanart.jpg".to_owned(), None, None),
                (ArtworkKind::Poster, "/poster.jpg".to_owned(), None, None),
                (
                    ArtworkKind::Poster,
                    "/season-specials-poster.jpg".to_owned(),
                    Some(0),
                    None
                ),
                (
                    ArtworkKind::Poster,
                    "/season01-poster.jpg".to_owned(),
                    Some(1),
                    None
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_season_poster_is_not_kept_for_a_season_sonarr_no_longer_lists() {
        let library = ShowLibrary::new().await;
        library.sources().folder.season_posters = vec![(
            2,
            found(
                &format!("{SHOW_FOLDER}/season02-poster.jpg"),
                "2026-10-28T12:31:00Z",
            )
            .unwrap(),
        )];
        library.sync_show().await.unwrap();
        let kept_while_listed = artwork(&library).await.len();

        with_sonarr_episodes(&library, |episodes| {
            episodes.retain(|x| x.season_number != 2)
        });
        library.sync_show().await.unwrap();

        assert_eq!(kept_while_listed, 1);
        assert!(artwork(&library).await.is_empty());
    }

    #[tokio::test]
    async fn a_later_sync_picks_up_changed_nfos_and_images() {
        let library = ShowLibrary::new().await;
        with_show_nfo(&library, Some(show_nfo()), "2026-10-29T11:00:00Z");
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.0,
            )]),
            "2026-10-29T11:00:00Z",
        );
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-28T12:30:00Z");
            sources.folder.fanart =
                found(&format!("{SHOW_FOLDER}/fanart.jpg"), "2026-10-28T12:30:00Z");
        }
        library.sync_show().await.unwrap();

        let ended = ShowNfo {
            status: Some("Ended".to_owned()),
            ..show_nfo()
        };
        with_show_nfo(&library, Some(ended), "2026-10-30T08:00:00Z");
        with_episode_nfo(
            &library,
            S01E01_FILE,
            Some(vec![episode_nfo(
                Some(1011),
                1,
                1,
                "Good News About Hell",
                8.2,
            )]),
            "2026-10-30T08:00:00Z",
        );
        {
            let mut sources = library.sources();
            sources.folder.poster =
                found(&format!("{SHOW_FOLDER}/poster.jpg"), "2026-10-30T08:00:00Z");
            sources.folder.fanart = None;
            sources.folder.season_posters = vec![(
                1,
                found(
                    &format!("{SHOW_FOLDER}/season01-poster.jpg"),
                    "2026-10-30T08:00:00Z",
                )
                .unwrap(),
            )];
            sources.episode_folders.get_mut(S01E01_FILE).unwrap().thumb = found(
                &S01E01_FILE.replace(".mkv", "-thumb.jpg"),
                "2026-10-30T08:00:00Z",
            );
        }
        library.sync_show().await.unwrap();

        assert_eq!(show(&library).await.status.as_deref(), Some("Ended"));
        assert_eq!(actors(&library).await.len(), 3);
        assert_eq!(episode(&library, 1, 1).await.rating, Some(8.2));
        assert_eq!(
            artwork(&library)
                .await
                .into_iter()
                .map(|x| (x.kind, x.path, x.modified_at))
                .collect::<Vec<_>>(),
            [
                (
                    ArtworkKind::Thumb,
                    S01E01_FILE.replace(".mkv", "-thumb.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{SHOW_FOLDER}/poster.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
                (
                    ArtworkKind::Poster,
                    format!("{SHOW_FOLDER}/season01-poster.jpg"),
                    at("2026-10-30T08:00:00Z")
                ),
            ]
        );
        assert_eq!(library.sources().nfo_reads, 2);
    }
}
