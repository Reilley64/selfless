use crate::DbContext;
use crate::Prober;
use crate::SourceHealth;
use crate::library_items;
use crate::library_items::Change;
use crate::media_files;
use crate::media_files::Inspected;
use crate::media_files::Inspector;
use crate::ports::EpisodeFolder;
use crate::ports::EpisodeNfo;
use crate::ports::FoundFile;
use crate::ports::GetSonarrShow;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ShowFolder;
use crate::ports::ShowNfo;
use crate::ports::SonarrEpisode;
use crate::ports::SonarrSeries;
use crate::ports::SonarrShow;
use domain::Actor;
use domain::Artwork;
use domain::ArtworkKind;
use domain::Episode;
use domain::MediaFile;
use domain::Season;
use domain::Show;
use domain::Source;
use domain::SyncProblem;
use domain::SyncProblemKind;
use futures_util::future::try_join_all;
use jiff::Timestamp;
use jiff::civil::Date;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use tracing::Instrument;
use tracing::Span;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_sonarr_show: GetSonarrShow,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to sync a show from Sonarr")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, sonarr_id: i64) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        get_sonarr_show,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
        source_health,
    } = dependencies.into();
    let inspector = Inspector {
        prober,
        list_subtitle_files,
    };
    async {
        let sonarr_show = get_sonarr_show.call(sonarr_id).await;
        match source_health
            .api_call(Source::Sonarr, sonarr_show)
            .map_err(Error::caused_by)?
        {
            Some(sonarr_show) => {
                save(
                    &mut db_context,
                    &inspector,
                    Readers {
                        read_show_folder: &read_show_folder,
                        read_show_nfo: &read_show_nfo,
                        read_episode_folder: &read_episode_folder,
                        read_episode_nfo: &read_episode_nfo,
                    },
                    sonarr_show,
                )
                .await
            }
            None => match Show::filter_by_sonarr_id(sonarr_id)
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
    .await
}

pub(crate) fn span() -> Span {
    tracing::info_span!("sync_show", otel.name = "sync sonarr show")
}

#[derive(Clone, Copy)]
pub(crate) struct Readers<'a> {
    pub(crate) read_show_folder: &'a ReadShowFolder,
    pub(crate) read_show_nfo: &'a ReadShowNfo,
    pub(crate) read_episode_folder: &'a ReadEpisodeFolder,
    pub(crate) read_episode_nfo: &'a ReadEpisodeNfo,
}

struct EpisodeFile {
    folder: EpisodeFolder,
}

enum Nfo<T> {
    Missing,
    Unchanged,
    Read(T, Timestamp),
    Unreadable(String),
}

#[derive(Clone, PartialEq)]
struct ShowMetadata {
    title: String,
    sort_title: Option<String>,
    overview: Option<String>,
    certification: Option<String>,
    premiered: Option<Date>,
    end_date: Option<Date>,
    status: Option<String>,
    network: Option<String>,
    genres: Vec<String>,
    rating: Option<f64>,
    nfo_modified_at: Option<Timestamp>,
}

impl ShowMetadata {
    fn stored(show: &Show) -> Self {
        Self {
            title: show.title.clone(),
            sort_title: show.sort_title.clone(),
            overview: show.overview.clone(),
            certification: show.certification.clone(),
            premiered: show.premiered,
            end_date: show.end_date,
            status: show.status.clone(),
            network: show.network.clone(),
            genres: show.genres.clone(),
            rating: show.rating,
            nfo_modified_at: show.nfo_modified_at,
        }
    }

    fn of(nfo: &Nfo<ShowNfo>, series: &SonarrSeries, stored: Option<&Show>) -> Self {
        match (nfo, stored) {
            (Nfo::Unchanged, Some(show)) => Self::stored(show),
            (Nfo::Read(nfo, modified_at), _) => Self {
                title: nfo.title.clone().unwrap_or_else(|| series.title.clone()),
                sort_title: nfo.sort_title.clone(),
                overview: nfo.plot.clone().or_else(|| series.overview.clone()),
                certification: nfo.certification.clone(),
                premiered: nfo.premiered,
                end_date: nfo.end_date,
                status: nfo.status.clone(),
                network: nfo.network.clone(),
                genres: if nfo.genres.is_empty() {
                    series.genres.clone()
                } else {
                    nfo.genres.clone()
                },
                rating: nfo.rating,
                nfo_modified_at: Some(*modified_at),
            },
            _ => Self {
                title: series.title.clone(),
                sort_title: None,
                overview: series.overview.clone(),
                certification: None,
                premiered: None,
                end_date: None,
                status: None,
                network: None,
                genres: series.genres.clone(),
                rating: None,
                nfo_modified_at: None,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
struct EpisodeMetadata {
    season_number: i64,
    number: i64,
    title: String,
    air_date: Option<Date>,
    overview: Option<String>,
    rating: Option<f64>,
    nfo_modified_at: Option<Timestamp>,
}

impl EpisodeMetadata {
    fn stored(episode: &Episode, season_number: i64) -> Self {
        Self {
            season_number,
            number: episode.number,
            title: episode.title.clone(),
            air_date: episode.air_date,
            overview: episode.overview.clone(),
            rating: episode.rating,
            nfo_modified_at: episode.nfo_modified_at,
        }
    }

    fn of(
        nfo: &Nfo<EpisodeNfo>,
        sonarr_episode: &SonarrEpisode,
        stored: Option<&EpisodeMetadata>,
    ) -> Self {
        match (nfo, stored) {
            (Nfo::Unchanged, Some(stored)) => stored.clone(),
            (Nfo::Read(nfo, modified_at), _) => Self {
                season_number: nfo.season_number.unwrap_or(sonarr_episode.season_number),
                number: nfo.episode_number.unwrap_or(sonarr_episode.episode_number),
                title: nfo
                    .title
                    .clone()
                    .unwrap_or_else(|| sonarr_episode.title.clone()),
                air_date: nfo.air_date.or(sonarr_episode.air_date),
                overview: nfo.plot.clone().or_else(|| sonarr_episode.overview.clone()),
                rating: nfo.rating,
                nfo_modified_at: Some(*modified_at),
            },
            _ => Self {
                season_number: sonarr_episode.season_number,
                number: sonarr_episode.episode_number,
                title: sonarr_episode.title.clone(),
                air_date: sonarr_episode.air_date,
                overview: sonarr_episode.overview.clone(),
                rating: None,
                nfo_modified_at: None,
            },
        }
    }
}

#[derive(PartialEq)]
struct FoundArtwork {
    season_id: Option<Uuid>,
    episode_id: Option<Uuid>,
    kind: ArtworkKind,
    file: FoundFile,
}

pub(crate) async fn save(
    db_context: &mut DbContext,
    inspector: &Inspector,
    readers: Readers<'_>,
    sonarr_show: SonarrShow,
) -> Result<(), Error> {
    let mut files = HashMap::new();
    for file in sonarr_show
        .episodes
        .iter()
        .filter_map(|x| x.episode_file.as_ref())
    {
        if !files.contains_key(&file.path) {
            files.insert(
                file.path.clone(),
                EpisodeFile {
                    folder: readers.read_episode_folder.call(&file.path).await,
                },
            );
        }
    }
    let season_numbers = sonarr_show
        .episodes
        .iter()
        .map(|x| x.season_number)
        .collect::<BTreeSet<_>>();
    let folder = readers
        .read_show_folder
        .call(
            &sonarr_show.series.path,
            season_numbers.into_iter().collect(),
        )
        .await;
    let stored = if files.is_empty() {
        HashMap::new()
    } else {
        MediaFile::filter(
            MediaFile::fields()
                .path()
                .in_list(files.keys().cloned().collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.path.clone(), x))
        .collect::<HashMap<_, _>>()
    };
    let inspected = try_join_all(files.keys().map(async |path| {
        let inspected = inspector.inspect(stored.get(path), path).await?;
        Ok::<_, media_files::Error>((path.clone(), inspected))
    }))
    .await
    .map_err(Error::caused_by)?
    .into_iter()
    .collect::<HashMap<_, _>>();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let change = sync(
        transaction.executor_mut(),
        sonarr_show,
        &files,
        inspected,
        folder,
        readers,
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
    delete(transaction.executor_mut(), show).await?;
    transaction.commit().await.map_err(Error::caused_by)?;
    library_items::count(Source::Sonarr, Change::Removed);
    Ok(())
}

async fn sync(
    db: &mut dyn toasty::Executor,
    sonarr_show: SonarrShow,
    files: &HashMap<String, EpisodeFile>,
    mut inspected: HashMap<String, Inspected>,
    folder: ShowFolder,
    readers: Readers<'_>,
) -> Result<Option<Change>, Error> {
    let series = sonarr_show.series;
    let stored = Show::filter_by_tvdb_id(series.tvdb_id)
        .first()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    let nfo = match folder.nfo {
        None => Nfo::Missing,
        Some(file)
            if stored
                .as_ref()
                .is_some_and(|x| x.nfo_modified_at == Some(file.modified_at)) =>
        {
            Nfo::Unchanged
        }
        Some(file) => match readers.read_show_nfo.call(&file.path).await {
            Ok(nfo) => Nfo::Read(nfo, file.modified_at),
            Err(error) => {
                warn_unreadable(&error, &file.path);
                Nfo::Unreadable(file.path)
            }
        },
    };
    let metadata = ShowMetadata::of(&nfo, &series, stored.as_ref());
    let (show, show_change) = match stored {
        Some(mut show) => {
            let had_nfo = show.nfo_modified_at.is_some();
            let changed = show.sonarr_id != series.sonarr_id
                || show.year != series.year
                || ShowMetadata::stored(&show) != metadata;
            if changed {
                show.update()
                    .sonarr_id(series.sonarr_id)
                    .year(series.year)
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
                    .nfo_modified_at(metadata.nfo_modified_at)
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
            }
            if had_nfo && !matches!(nfo, Nfo::Unchanged) {
                Actor::filter_by_show_id(show.id)
                    .delete()
                    .exec(db)
                    .await
                    .map_err(Error::caused_by)?;
            }
            (show, changed.then_some(Change::Changed))
        }
        None => (
            toasty::create!(Show {
                tvdb_id: series.tvdb_id,
                sonarr_id: series.sonarr_id,
                year: series.year,
                title: metadata.title,
                sort_title: metadata.sort_title,
                overview: metadata.overview,
                certification: metadata.certification,
                premiered: metadata.premiered,
                end_date: metadata.end_date,
                status: metadata.status,
                network: metadata.network,
                genres: metadata.genres,
                rating: metadata.rating,
                nfo_modified_at: metadata.nfo_modified_at,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?,
            Some(Change::Added),
        ),
    };
    if let Nfo::Read(nfo, _) = &nfo {
        for actor in &nfo.actors {
            toasty::create!(Actor {
                movie_id: None,
                show_id: Some(show.id),
                name: actor.name.clone(),
                role: actor.role.clone(),
                order: actor.order,
            })
            .exec(db)
            .await
            .map_err(Error::caused_by)?;
        }
    }
    let mut changed = show_change.is_some();
    let mut artwork = [
        (ArtworkKind::Poster, folder.poster),
        (ArtworkKind::Fanart, folder.fanart),
        (ArtworkKind::Clearlogo, folder.clearlogo),
    ]
    .into_iter()
    .filter_map(|(kind, file)| {
        Some(FoundArtwork {
            season_id: None,
            episode_id: None,
            kind,
            file: file?,
        })
    })
    .collect::<Vec<_>>();
    let mut seasons = show
        .seasons()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.number, x))
        .collect::<HashMap<_, _>>();
    let season_numbers = seasons
        .values()
        .map(|x| (x.id, x.number))
        .collect::<HashMap<_, _>>();
    let mut episodes = episodes_of(db, &seasons.values().map(|x| x.id).collect::<Vec<_>>())
        .await?
        .into_iter()
        .map(|x| (x.tvdb_id, x))
        .collect::<HashMap<_, _>>();
    let old_media_files = episodes
        .values()
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    let mut synced_seasons = HashSet::new();
    let mut media_files = HashSet::new();
    let mut nfo_roots = HashMap::new();
    let mut unreadable_nfos = HashMap::new();
    let mut saved = HashMap::new();
    for sonarr_episode in sonarr_show.episodes {
        let file = sonarr_episode
            .episode_file
            .as_ref()
            .and_then(|x| files.get(&x.path));
        let stored = episodes.remove(&sonarr_episode.tvdb_id).map(|x| {
            let metadata = EpisodeMetadata::stored(&x, season_numbers[&x.season_id]);
            (x, metadata)
        });
        let nfo = match file.and_then(|x| x.folder.nfo.as_ref()) {
            None => Nfo::Missing,
            Some(nfo_file)
                if stored
                    .as_ref()
                    .is_some_and(|(x, _)| x.nfo_modified_at == Some(nfo_file.modified_at)) =>
            {
                Nfo::Unchanged
            }
            Some(nfo_file) => {
                if !nfo_roots.contains_key(&nfo_file.path) {
                    let roots = readers.read_episode_nfo.call(&nfo_file.path).await;
                    if let Err(error) = &roots {
                        warn_unreadable(error, &nfo_file.path);
                    }
                    nfo_roots.insert(nfo_file.path.clone(), roots.ok());
                }
                match &nfo_roots[&nfo_file.path] {
                    None => Nfo::Unreadable(nfo_file.path.clone()),
                    Some(roots) => match roots.iter().find(|x| {
                        x.sonarr_id == Some(sonarr_episode.sonarr_id)
                            || x.sonarr_id.is_none()
                                && x.season_number == Some(sonarr_episode.season_number)
                                && x.episode_number == Some(sonarr_episode.episode_number)
                    }) {
                        Some(root) => Nfo::Read(root.clone(), nfo_file.modified_at),
                        None => Nfo::Missing,
                    },
                }
            }
        };
        let metadata = EpisodeMetadata::of(&nfo, &sonarr_episode, stored.as_ref().map(|(_, x)| x));
        let season_id = match seasons.get(&metadata.season_number) {
            Some(season) => season.id,
            None => {
                let season = toasty::create!(Season {
                    show_id: show.id,
                    number: metadata.season_number,
                })
                .exec(db)
                .await
                .map_err(Error::caused_by)?;
                changed = true;
                let id = season.id;
                seasons.insert(metadata.season_number, season);
                id
            }
        };
        synced_seasons.insert(metadata.season_number);
        let (media_file_id, visible) = match &sonarr_episode.episode_file {
            Some(sonarr_file) => {
                let (id, visible) = match inspected.remove(&sonarr_file.path) {
                    Some(inspected) => {
                        let visible = inspected.on_disk();
                        let (id, file_changed) =
                            media_files::save(db, &sonarr_file.path, inspected)
                                .await
                                .map_err(Error::caused_by)?;
                        changed |= file_changed;
                        saved.insert(sonarr_file.path.clone(), (id, visible));
                        (id, visible)
                    }
                    None => saved[&sonarr_file.path],
                };
                media_files.insert(id);
                if let Nfo::Unreadable(path) = &nfo {
                    unreadable_nfos.insert(id, path.clone());
                }
                (Some(id), visible)
            }
            None => (None, false),
        };
        let date_added = sonarr_episode.episode_file.map(|x| x.date_added);
        let episode_id = match stored {
            Some((mut episode, stored_metadata)) => {
                let added_at = episode.added_at.or(date_added);
                if episode.season_id != season_id
                    || stored_metadata != metadata
                    || episode.added_at != added_at
                    || episode.media_file_id != media_file_id
                    || episode.visible != visible
                {
                    changed = true;
                    episode
                        .update()
                        .season_id(season_id)
                        .number(metadata.number)
                        .title(metadata.title)
                        .air_date(metadata.air_date)
                        .overview(metadata.overview)
                        .rating(metadata.rating)
                        .nfo_modified_at(metadata.nfo_modified_at)
                        .added_at(added_at)
                        .media_file_id(media_file_id)
                        .visible(visible)
                        .exec(db)
                        .await
                        .map_err(Error::caused_by)?;
                }
                episode.id
            }
            None => {
                changed = true;
                toasty::create!(Episode {
                    season_id,
                    tvdb_id: sonarr_episode.tvdb_id,
                    number: metadata.number,
                    title: metadata.title,
                    air_date: metadata.air_date,
                    overview: metadata.overview,
                    rating: metadata.rating,
                    nfo_modified_at: metadata.nfo_modified_at,
                    added_at: date_added,
                    media_file_id,
                    visible,
                })
                .exec(db)
                .await
                .map_err(Error::caused_by)?
                .id
            }
        };
        if let Some(thumb) = file.and_then(|x| x.folder.thumb.clone()) {
            artwork.push(FoundArtwork {
                season_id: None,
                episode_id: Some(episode_id),
                kind: ArtworkKind::Thumb,
                file: thumb,
            });
        }
    }
    for (number, file) in folder.season_posters {
        if let Some(season) = seasons.get(&number)
            && synced_seasons.contains(&number)
        {
            artwork.push(FoundArtwork {
                season_id: Some(season.id),
                episode_id: None,
                kind: ArtworkKind::Poster,
                file,
            });
        }
    }
    let episodes = episodes.into_values().map(|x| x.id).collect::<Vec<_>>();
    let seasons = seasons
        .into_values()
        .filter(|x| !synced_seasons.contains(&x.number))
        .map(|x| x.id)
        .collect::<Vec<_>>();
    let removed_media_files = old_media_files
        .difference(&media_files)
        .copied()
        .collect::<Vec<_>>();
    changed |= !episodes.is_empty() || !seasons.is_empty() || !removed_media_files.is_empty();
    delete_episodes(db, episodes).await?;
    delete_seasons(db, seasons).await?;
    delete_media_files(db, removed_media_files).await?;
    changed |= save_artwork(db, show.id, artwork).await?;
    let show_nfo = match &nfo {
        Nfo::Unreadable(path) => Some(path.as_str()),
        _ => None,
    };
    save_unreadable_nfos(db, show.id, show_nfo, &media_files, unreadable_nfos).await?;
    Ok(show_change.or(changed.then_some(Change::Changed)))
}

fn warn_unreadable(error: &crate::ports::UnreadableNfo, path: &str) {
    tracing::warn!(
        error = error as &dyn std::error::Error,
        file.path = path,
        "failed to read an NFO file"
    );
}

async fn save_artwork(
    db: &mut dyn toasty::Executor,
    show_id: Uuid,
    found: Vec<FoundArtwork>,
) -> Result<bool, Error> {
    let stored = Artwork::filter_by_show_id(show_id)
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| FoundArtwork {
            season_id: x.season_id,
            episode_id: x.episode_id,
            kind: x.kind,
            file: FoundFile {
                path: x.path,
                modified_at: x.modified_at,
            },
        })
        .collect::<Vec<_>>();
    if stored.len() == found.len() && found.iter().all(|x| stored.contains(x)) {
        return Ok(false);
    }
    Artwork::filter_by_show_id(show_id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    for artwork in found {
        toasty::create!(Artwork {
            movie_id: None,
            show_id: Some(show_id),
            season_id: artwork.season_id,
            episode_id: artwork.episode_id,
            kind: artwork.kind,
            path: artwork.file.path,
            modified_at: artwork.file.modified_at,
        })
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    }
    Ok(true)
}

async fn save_unreadable_nfos(
    db: &mut dyn toasty::Executor,
    show_id: Uuid,
    show_nfo: Option<&str>,
    media_files: &HashSet<Uuid>,
    episode_nfos: HashMap<Uuid, String>,
) -> Result<(), Error> {
    let mut stored = SyncProblem::filter_by_show_id(show_id)
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    if !media_files.is_empty() {
        stored.extend(
            SyncProblem::filter(
                SyncProblem::fields()
                    .media_file_id()
                    .in_list(media_files.iter().copied().collect::<Vec<_>>())
                    .and(
                        SyncProblem::fields()
                            .kind()
                            .eq(SyncProblemKind::UnreadableNfo),
                    ),
            )
            .exec(db)
            .await
            .map_err(Error::caused_by)?,
        );
    }
    let found = show_nfo
        .map(|path| (None, path.to_owned()))
        .into_iter()
        .chain(episode_nfos.into_iter().map(|(id, path)| (Some(id), path)))
        .collect::<HashSet<_>>();
    let stored_found = stored
        .iter()
        .map(|x| (x.media_file_id, x.path.clone()))
        .collect::<HashSet<_>>();
    if stored_found == found {
        return Ok(());
    }
    for problem in stored {
        problem.delete().exec(db).await.map_err(Error::caused_by)?;
    }
    for (media_file_id, path) in found {
        toasty::create!(SyncProblem {
            movie_id: None,
            media_file_id,
            show_id: media_file_id.is_none().then_some(show_id),
            kind: SyncProblemKind::UnreadableNfo,
            path,
        })
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    }
    Ok(())
}

async fn delete(db: &mut dyn toasty::Executor, show: Show) -> Result<(), Error> {
    let seasons = show
        .seasons()
        .exec(db)
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    let media_files = episodes_of(db, &seasons)
        .await?
        .into_iter()
        .filter_map(|x| x.media_file_id)
        .collect::<HashSet<_>>();
    Actor::filter_by_show_id(show.id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    Artwork::filter_by_show_id(show.id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    SyncProblem::filter_by_show_id(show.id)
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)?;
    show.delete().exec(db).await.map_err(Error::caused_by)?;
    delete_media_files(db, media_files.into_iter().collect()).await
}

async fn episodes_of(
    db: &mut dyn toasty::Executor,
    season_ids: &[Uuid],
) -> Result<Vec<Episode>, Error> {
    if season_ids.is_empty() {
        return Ok(Vec::new());
    }
    Episode::filter(Episode::fields().season_id().in_list(season_ids))
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_episodes(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    Episode::filter(Episode::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_seasons(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    Season::filter(Season::fields().id().in_list(ids))
        .delete()
        .exec(db)
        .await
        .map_err(Error::caused_by)
}

async fn delete_media_files(db: &mut dyn toasty::Executor, ids: Vec<Uuid>) -> Result<(), Error> {
    media_files::delete(db, ids).await.map_err(Error::caused_by)
}
