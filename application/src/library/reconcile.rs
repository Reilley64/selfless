mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_LIBRARY_SOURCE;
use crate::attributes::SELFLESS_RECONCILE_RESULT;
use crate::library::SourceHealth;
use crate::library::prober::CONCURRENT_PROBES;
use crate::library::source_states;
use crate::library::sync_movie;
use crate::library::sync_movie::MovieFiles;
use crate::library::sync_show;
use crate::library::sync_show::ShowFiles;
use crate::library::sync_status::ReconcileProgress;
use crate::metrics::SELFLESS_RECONCILE_DURATION;
use crate::ports::GetSonarrEpisodes;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrShows;
use crate::ports::SonarrShow;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::HeldRemoval;
use domain::Movie;
use domain::ReconcileResult;
use domain::Show;
use domain::Source;
use futures_util::StreamExt;
use futures_util::stream;
use opentelemetry::KeyValue;
use opentelemetry::global;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;
use tracing::Instrument;
use tracing::field;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<Output, Error> {
    let Dependencies {
        check_radarr_kodi_metadata,
        check_sonarr_kodi_metadata,
        clock,
        mut db_context,
        find_media_file,
        get_sonarr_episodes,
        list_radarr_movies,
        list_sonarr_shows,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_movie_folder,
        read_movie_nfo,
        read_show_folder,
        read_show_nfo,
        reconciles,
        source_health,
    } = dependencies.into();
    let reconcile = reconciles.source(source);
    let Ok(_running) = reconcile.running.clone().try_lock_owned() else {
        return Ok(Output);
    };
    let progress = &reconcile.progress;
    *progress.lock().unwrap() = Some(ReconcileProgress {
        imported: 0,
        total: None,
    });
    let started_at = clock.call();
    let start = Instant::now();
    let span = tracing::info_span!(
        "reconcile",
        otel.name = format!("reconcile {}", source.name()),
        selfless.library.source = source.name(),
        selfless.reconcile.result = field::Empty,
    );
    let reconciled = async {
        let (failed_metadata_calls, listing) = match source {
            Source::Radarr => (
                record_kodi_metadata(
                    &source_health,
                    source,
                    check_radarr_kodi_metadata.call().await,
                ),
                sync_movies(
                    &mut db_context,
                    &MovieFiles {
                        find_media_file,
                        list_subtitle_files,
                        prober,
                        read_movie_folder,
                        read_movie_nfo,
                    },
                    &list_radarr_movies,
                    &source_health,
                    progress,
                )
                .await?,
            ),
            Source::Sonarr => (
                record_kodi_metadata(
                    &source_health,
                    source,
                    check_sonarr_kodi_metadata.call().await,
                ),
                sync_shows(
                    &mut db_context,
                    &ShowFiles {
                        find_media_file,
                        list_subtitle_files,
                        prober,
                        read_episode_folder,
                        read_episode_nfo,
                        read_show_folder,
                        read_show_nfo,
                    },
                    &list_sonarr_shows,
                    &get_sonarr_episodes,
                    &source_health,
                    progress,
                )
                .await?,
            ),
        };
        let failed_calls = failed_metadata_calls + listing.failed_calls;
        if failed_calls == 0 {
            remove_or_hold(&mut db_context, source, &listing).await?;
        }
        if failed_calls > 0 {
            return Err(Error::caused_by(format!(
                "{failed_calls} calls to {} failed, so nothing was removed",
                source.name()
            )));
        }
        Ok(())
    }
    .instrument(span.clone())
    .await;
    *progress.lock().unwrap() = None;
    let duration = start.elapsed();
    let result = match reconciled {
        Ok(()) => ReconcileResult::Succeeded,
        Err(error) => {
            span.in_scope(|| {
                tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    selfless.library.source = source.name(),
                    "failed to reconcile the library"
                );
            });
            ReconcileResult::Failed
        }
    };
    span.record(SELFLESS_RECONCILE_RESULT, result.name());
    global::meter("selfless")
        .f64_histogram(SELFLESS_RECONCILE_DURATION)
        .with_unit("s")
        .build()
        .record(
            duration.as_secs_f64(),
            &[
                KeyValue::new(SELFLESS_LIBRARY_SOURCE, source.name()),
                KeyValue::new(SELFLESS_RECONCILE_RESULT, result.name()),
            ],
        );
    source_states::upsert(&mut db_context, source)
        .instrument(span.clone())
        .await
        .map_err(Error::caused_by)?
        .update()
        .last_reconcile_started_at(Some(started_at))
        .last_reconcile_duration_ms(Some(
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        ))
        .last_reconcile_result(Some(result))
        .exec(db_context.executor_mut())
        .instrument(span)
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

fn record_kodi_metadata(
    source_health: &SourceHealth,
    source: Source,
    kodi_metadata: Result<bool, impl std::error::Error + 'static>,
) -> usize {
    match source_health.api_call(source, kodi_metadata) {
        Ok(enabled) => {
            source_health.set_kodi_metadata(source, enabled);
            0
        }
        Err(error) => {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                selfless.library.source = source.name(),
                "failed to read the metadata settings"
            );
            1
        }
    }
}

struct Listing {
    listed: HashSet<i64>,
    total: usize,
    absent: Vec<Uuid>,
    failed_calls: usize,
}

async fn sync_movies(
    db_context: &mut DbContext,
    files: &MovieFiles,
    list_radarr_movies: &ListRadarrMovies,
    source_health: &SourceHealth,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<Listing, Error> {
    let radarr_movies = source_health
        .api_call(Source::Radarr, list_radarr_movies.call().await)
        .map_err(Error::caused_by)?;
    let listed = radarr_movies
        .iter()
        .map(|x| x.tmdb_id)
        .collect::<HashSet<_>>();
    set_total(progress, radarr_movies.len());
    let stored_movies = db_context
        .movies()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let total = stored_movies.len();
    let absent = stored_movies
        .into_iter()
        .filter(|x| !listed.contains(&x.tmdb_id))
        .map(|x| x.id)
        .collect();
    let stored = db_context
        .media_files()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.path.clone(), x))
        .collect::<HashMap<_, _>>();
    let mut inspections = stream::iter(radarr_movies)
        .map(|radarr_movie| {
            let span = sync_movie::span();
            let stored = radarr_movie
                .movie_file
                .as_ref()
                .and_then(|x| stored.get(&x.path));
            async move {
                let inspected = sync_movie::inspect(files, stored, &radarr_movie)
                    .instrument(span.clone())
                    .await;
                (radarr_movie, inspected, span)
            }
        })
        .buffered(CONCURRENT_PROBES);
    while let Some((radarr_movie, inspected, span)) = inspections.next().await {
        let inspected = inspected.map_err(Error::caused_by)?;
        sync_movie::save(db_context, files, radarr_movie, inspected)
            .instrument(span)
            .await
            .map_err(Error::caused_by)?;
        imported(progress);
    }
    Ok(Listing {
        listed,
        total,
        absent,
        failed_calls: 0,
    })
}

async fn sync_shows(
    db_context: &mut DbContext,
    files: &ShowFiles,
    list_sonarr_shows: &ListSonarrShows,
    get_sonarr_episodes: &GetSonarrEpisodes,
    source_health: &SourceHealth,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<Listing, Error> {
    let listings = source_health
        .api_call(Source::Sonarr, list_sonarr_shows.call().await)
        .map_err(Error::caused_by)?;
    let listed = listings.iter().map(|x| x.tvdb_id).collect::<HashSet<_>>();
    set_total(progress, listings.len());
    let stored_shows = db_context
        .shows()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let total = stored_shows.len();
    let absent = stored_shows
        .into_iter()
        .filter(|x| !listed.contains(&x.tvdb_id))
        .map(|x| x.id)
        .collect();
    let mut failed_calls = 0;
    for listing in listings {
        let span = sync_show::span();
        let episodes = get_sonarr_episodes
            .call(listing.sonarr_id)
            .instrument(span.clone())
            .await;
        match source_health.api_call(Source::Sonarr, episodes) {
            Ok(episodes) => {
                sync_show::save(db_context, files, SonarrShow { listing, episodes })
                    .instrument(span)
                    .await
                    .map_err(Error::caused_by)?;
            }
            Err(error) => {
                span.in_scope(|| {
                    tracing::warn!(
                        error = &error as &dyn std::error::Error,
                        "failed to read a show's episodes from Sonarr"
                    );
                });
                failed_calls += 1;
            }
        }
        imported(progress);
    }
    Ok(Listing {
        listed,
        total,
        absent,
        failed_calls,
    })
}

fn set_total(progress: &Mutex<Option<ReconcileProgress>>, total: usize) {
    *progress.lock().unwrap() = Some(ReconcileProgress {
        imported: 0,
        total: Some(total),
    });
}

fn imported(progress: &Mutex<Option<ReconcileProgress>>) {
    if let Some(progress) = progress.lock().unwrap().as_mut() {
        progress.imported += 1;
    }
}

async fn remove_or_hold(
    db_context: &mut DbContext,
    source: Source,
    listing: &Listing,
) -> Result<(), Error> {
    db_context
        .held_removals()
        .filter(HeldRemoval::fields().source().eq(source))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if HeldRemoval::holds(listing.absent.len(), listing.total) {
        tracing::warn!(
            selfless.library.source = source.name(),
            "held a removal of more than half of a source's items"
        );
        db_context
            .held_removals()
            .create()
            .source(source)
            .item_ids(listing.absent.clone())
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        return Ok(());
    }
    remove(db_context, source, listing.absent.clone(), &listing.listed).await
}

pub(crate) async fn remove(
    db_context: &mut DbContext,
    source: Source,
    ids: Vec<Uuid>,
    listed: &HashSet<i64>,
) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    match source {
        Source::Radarr => {
            let movies = db_context
                .movies()
                .filter(Movie::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .into_iter()
                .filter(|x| !listed.contains(&x.tmdb_id))
                .collect();
            sync_movie::remove(db_context, movies)
                .await
                .map_err(Error::caused_by)
        }
        Source::Sonarr => {
            let shows = db_context
                .shows()
                .filter(Show::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            for show in shows.into_iter().filter(|x| !listed.contains(&x.tvdb_id)) {
                sync_show::remove(db_context, show)
                    .await
                    .map_err(Error::caused_by)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Reconciles;
    use crate::library::sync_status;
    use crate::library::sync_status::ApiCall;
    use crate::library::sync_status::ReconcileProgress;
    use crate::ports::CheckRadarrKodiMetadata;
    use crate::ports::CheckSonarrKodiMetadata;
    use crate::ports::Clock;
    use crate::ports::FindMediaFile;
    use crate::ports::FoundMediaFile;
    use crate::ports::RadarrError;
    use crate::ports::RadarrMovie;
    use crate::ports::RadarrMovieFile;
    use crate::ports::SonarrEpisode;
    use crate::ports::SonarrEpisodeFile;
    use crate::ports::SonarrError;
    use crate::ports::SonarrShowListing;
    use crate::test::empty_episode_folder;
    use crate::test::empty_movie_folder;
    use crate::test::empty_show_folder;
    use crate::test::failing_prober;
    use crate::test::mock_db_context;
    use crate::test::no_subtitle_files;
    use crate::test::unreadable_episode_nfo;
    use crate::test::unreadable_movie_nfo;
    use crate::test::unreadable_show_nfo;
    use jiff::Timestamp;
    use std::sync::Arc;
    use test_fixtures::FakeClock;
    use tokio::sync::Notify;

    const ARRIVAL: i64 = 42;
    const DUNE: i64 = 43;
    const SICARIO: i64 = 44;
    const BLADE_RUNNER: i64 = 45;
    const SEVERANCE: i64 = 7;
    const ANDOR: i64 = 8;

    fn title(id: i64) -> &'static str {
        match id {
            ARRIVAL => "Arrival",
            DUNE => "Dune",
            SICARIO => "Sicario",
            BLADE_RUNNER => "Blade Runner 2049",
            SEVERANCE => "Severance",
            ANDOR => "Andor",
            _ => unreachable!(),
        }
    }

    fn radarr_movie(tmdb_id: i64) -> RadarrMovie {
        RadarrMovie {
            radarr_id: tmdb_id,
            tmdb_id,
            title: title(tmdb_id).to_owned(),
            year: 2016,
            overview: None,
            genres: Vec::new(),
            movie_file: Some(RadarrMovieFile {
                path: format!("/movies/{}.mkv", title(tmdb_id)),
                date_added: Timestamp::UNIX_EPOCH,
            }),
        }
    }

    fn sonarr_listing(sonarr_id: i64) -> SonarrShowListing {
        SonarrShowListing {
            sonarr_id,
            tvdb_id: sonarr_id,
            path: format!("/tv/{}", title(sonarr_id)),
            title: title(sonarr_id).to_owned(),
            year: None,
            genres: Vec::new(),
            overview: None,
        }
    }

    fn sonarr_episodes(sonarr_id: i64) -> Vec<SonarrEpisode> {
        vec![SonarrEpisode {
            sonarr_id,
            tvdb_id: sonarr_id,
            season_number: 1,
            episode_number: 1,
            title: "Pilot".to_owned(),
            air_date: None,
            overview: None,
            episode_file: Some(SonarrEpisodeFile {
                path: format!("/tv/{}/S01E01.mkv", title(sonarr_id)),
                date_added: Timestamp::UNIX_EPOCH,
            }),
        }]
    }

    #[derive(Clone)]
    struct Library {
        db_context: DbContext,
        clock: FakeClock,
        reconciles: Reconciles,
        source_health: SourceHealth,
        radarr: Arc<Mutex<Result<Vec<RadarrMovie>, &'static str>>>,
        sonarr: Arc<Mutex<Result<Vec<i64>, &'static str>>>,
        failing_episodes: Arc<Mutex<HashSet<i64>>>,
        held_episodes: Arc<Mutex<Option<HeldEpisodes>>>,
        radarr_kodi: Arc<Mutex<Result<bool, &'static str>>>,
        sonarr_kodi: Arc<Mutex<Result<bool, &'static str>>>,
    }

    #[derive(Clone)]
    struct HeldEpisodes {
        sonarr_id: i64,
        reached: Arc<Notify>,
        released: Arc<Notify>,
    }

    impl Library {
        async fn new() -> Self {
            Self {
                db_context: mock_db_context().await,
                clock: FakeClock::new(),
                reconciles: Reconciles::default(),
                source_health: SourceHealth::default(),
                radarr: Arc::new(Mutex::new(Ok(Vec::new()))),
                sonarr: Arc::new(Mutex::new(Ok(Vec::new()))),
                failing_episodes: Arc::default(),
                held_episodes: Arc::default(),
                radarr_kodi: Arc::new(Mutex::new(Ok(true))),
                sonarr_kodi: Arc::new(Mutex::new(Ok(true))),
            }
        }

        fn radarr_lists(&self, ids: &[i64]) {
            self.radarr_lists_movies(ids.iter().copied().map(radarr_movie).collect());
        }

        fn radarr_lists_movies(&self, movies: Vec<RadarrMovie>) {
            *self.radarr.lock().unwrap() = Ok(movies);
        }

        fn sonarr_lists(&self, ids: &[i64]) {
            *self.sonarr.lock().unwrap() = Ok(ids.to_vec());
        }

        fn dependencies(&self) -> Dependencies {
            let radarr = self.radarr.clone();
            let sonarr = self.sonarr.clone();
            let failing_episodes = self.failing_episodes.clone();
            let held_episodes = self.held_episodes.clone();
            let radarr_kodi = self.radarr_kodi.clone();
            let sonarr_kodi = self.sonarr_kodi.clone();
            Dependencies {
                check_radarr_kodi_metadata: CheckRadarrKodiMetadata::new(move || {
                    let enabled = *radarr_kodi.lock().unwrap();
                    Box::pin(async move { enabled.map_err(RadarrError::caused_by) })
                }),
                check_sonarr_kodi_metadata: CheckSonarrKodiMetadata::new(move || {
                    let enabled = *sonarr_kodi.lock().unwrap();
                    Box::pin(async move { enabled.map_err(SonarrError::caused_by) })
                }),
                clock: Clock::new(self.clock.reader()),
                db_context: self.db_context.clone(),
                find_media_file: FindMediaFile::new(|_| {
                    Box::pin(async {
                        Some(FoundMediaFile {
                            size: 1,
                            modified_at: Timestamp::UNIX_EPOCH,
                        })
                    })
                }),
                get_sonarr_episodes: GetSonarrEpisodes::new(move |sonarr_id| {
                    let failing = failing_episodes.lock().unwrap().contains(&sonarr_id);
                    let held = held_episodes
                        .lock()
                        .unwrap()
                        .clone()
                        .filter(|x| x.sonarr_id == sonarr_id);
                    Box::pin(async move {
                        if let Some(held) = held {
                            held.reached.notify_one();
                            held.released.notified().await;
                        }
                        if failing {
                            return Err(SonarrError::caused_by("500 Internal Server Error"));
                        }
                        Ok(sonarr_episodes(sonarr_id))
                    })
                }),
                list_radarr_movies: ListRadarrMovies::new(move || {
                    let movies = radarr.lock().unwrap().clone();
                    Box::pin(async move { movies.map_err(RadarrError::caused_by) })
                }),
                list_sonarr_shows: ListSonarrShows::new(move || {
                    let ids = sonarr.lock().unwrap().clone();
                    Box::pin(async move {
                        ids.map(|x| x.into_iter().map(sonarr_listing).collect())
                            .map_err(SonarrError::caused_by)
                    })
                }),
                list_subtitle_files: no_subtitle_files(),
                prober: failing_prober(),
                read_episode_folder: empty_episode_folder(),
                read_episode_nfo: unreadable_episode_nfo(),
                read_movie_folder: empty_movie_folder(),
                read_movie_nfo: unreadable_movie_nfo(),
                read_show_folder: empty_show_folder(),
                read_show_nfo: unreadable_show_nfo(),
                reconciles: self.reconciles.clone(),
                source_health: self.source_health.clone(),
            }
        }

        async fn reconcile(&self, source: Source) -> Option<ReconcileResult> {
            call(self.dependencies(), source).await.unwrap();
            let mut db_context = self.db_context.clone();
            db_context
                .source_states()
                .filter(domain::SourceState::fields().source().eq(source))
                .one()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .last_reconcile_result
        }

        async fn status(&self) -> sync_status::Output {
            sync_status::call(sync_status::Dependencies {
                db_context: self.db_context.clone(),
                reconciles: self.reconciles.clone(),
                source_health: self.source_health.clone(),
            })
            .await
            .unwrap()
        }

        async fn movie_titles(&self) -> Vec<String> {
            let mut db_context = self.db_context.clone();
            let mut titles = db_context
                .movies()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .into_iter()
                .map(|x| x.title)
                .collect::<Vec<_>>();
            titles.sort();
            titles
        }

        async fn show_titles(&self) -> Vec<String> {
            let mut db_context = self.db_context.clone();
            let mut titles = db_context
                .shows()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .into_iter()
                .map(|x| x.title)
                .collect::<Vec<_>>();
            titles.sort();
            titles
        }
    }

    #[tokio::test]
    async fn a_reconcile_adds_new_movies_and_removes_movies_radarr_no_longer_lists() {
        let library = Library::new().await;
        library.radarr_lists(&[ARRIVAL, DUNE, SICARIO]);
        library.reconcile(Source::Radarr).await;

        library.radarr_lists(&[ARRIVAL, DUNE, BLADE_RUNNER]);
        let result = library.reconcile(Source::Radarr).await;

        assert_eq!(result, Some(ReconcileResult::Succeeded));
        assert_eq!(
            library.movie_titles().await,
            ["Arrival", "Blade Runner 2049", "Dune"]
        );
    }

    #[tokio::test]
    async fn a_reconcile_records_when_it_started_and_its_result() {
        let library = Library::new().await;
        library.radarr_lists(&[ARRIVAL]);

        library.reconcile(Source::Radarr).await;

        let last_reconcile = library.status().await.radarr.last_reconcile.unwrap();
        assert_eq!(last_reconcile.started_at, library.clock.now());
        assert_eq!(last_reconcile.result, ReconcileResult::Succeeded);
    }

    #[tokio::test]
    async fn a_reconcile_re_syncs_a_movie_whose_radarr_data_changed() {
        let library = Library::new().await;
        library.radarr_lists(&[ARRIVAL]);
        library.reconcile(Source::Radarr).await;

        let mut arrival = radarr_movie(ARRIVAL);
        arrival.title = "Arrival (Extended)".to_owned();
        library.radarr_lists_movies(vec![arrival]);
        library.reconcile(Source::Radarr).await;

        assert_eq!(library.movie_titles().await, ["Arrival (Extended)"]);
    }

    #[tokio::test]
    async fn a_failed_listing_call_removes_nothing() {
        let library = Library::new().await;
        library.radarr_lists(&[ARRIVAL, DUNE]);
        library.reconcile(Source::Radarr).await;

        *library.radarr.lock().unwrap() = Err("500 Internal Server Error");
        let result = library.reconcile(Source::Radarr).await;

        assert_eq!(result, Some(ReconcileResult::Failed));
        assert_eq!(library.movie_titles().await, ["Arrival", "Dune"]);
    }

    #[tokio::test]
    async fn a_failed_metadata_call_removes_nothing_but_still_adds_movies() {
        let library = Library::new().await;
        library.radarr_lists(&[ARRIVAL, DUNE]);
        library.reconcile(Source::Radarr).await;

        library.radarr_lists(&[ARRIVAL, SICARIO]);
        *library.radarr_kodi.lock().unwrap() = Err("500 Internal Server Error");
        let result = library.reconcile(Source::Radarr).await;

        assert_eq!(result, Some(ReconcileResult::Failed));
        assert_eq!(library.movie_titles().await, ["Arrival", "Dune", "Sicario"]);
    }

    #[tokio::test]
    async fn a_failed_episodes_call_removes_no_show_until_every_call_succeeds() {
        let library = Library::new().await;
        library.sonarr_lists(&[SEVERANCE, ANDOR]);
        library.reconcile(Source::Sonarr).await;

        library.sonarr_lists(&[SEVERANCE]);
        library.failing_episodes.lock().unwrap().insert(SEVERANCE);
        let failed = library.reconcile(Source::Sonarr).await;
        let after_failure = library.show_titles().await;
        library.failing_episodes.lock().unwrap().clear();
        let succeeded = library.reconcile(Source::Sonarr).await;

        assert_eq!(failed, Some(ReconcileResult::Failed));
        assert_eq!(after_failure, ["Andor", "Severance"]);
        assert_eq!(succeeded, Some(ReconcileResult::Succeeded));
        assert_eq!(library.show_titles().await, ["Severance"]);
    }

    #[tokio::test]
    async fn the_sync_status_shows_imported_n_of_m_while_a_reconcile_runs() {
        let library = Library::new().await;
        library.sonarr_lists(&[SEVERANCE, ANDOR]);
        let held = HeldEpisodes {
            sonarr_id: ANDOR,
            reached: Arc::new(Notify::new()),
            released: Arc::new(Notify::new()),
        };
        *library.held_episodes.lock().unwrap() = Some(held.clone());

        let reconciling = tokio::spawn(call(library.dependencies(), Source::Sonarr));
        held.reached.notified().await;
        let running = library.status().await.sonarr.reconcile_progress;
        let shows_while_running = library.show_titles().await;
        held.released.notify_one();
        reconciling.await.unwrap().unwrap();

        assert_eq!(
            running,
            Some(ReconcileProgress {
                imported: 1,
                total: Some(2),
            })
        );
        assert_eq!(shows_while_running, ["Severance"]);
        assert_eq!(library.status().await.sonarr.reconcile_progress, None);
        assert_eq!(library.show_titles().await, ["Andor", "Severance"]);
    }

    #[tokio::test]
    async fn each_reconcile_records_whether_kodi_metadata_is_on_in_radarr_and_sonarr() {
        let library = Library::new().await;
        *library.sonarr_kodi.lock().unwrap() = Ok(false);
        library.reconcile(Source::Radarr).await;
        library.reconcile(Source::Sonarr).await;
        let first = library.status().await;

        *library.radarr_kodi.lock().unwrap() = Ok(false);
        *library.sonarr_kodi.lock().unwrap() = Ok(true);
        library.reconcile(Source::Radarr).await;
        library.reconcile(Source::Sonarr).await;
        let second = library.status().await;

        assert_eq!(first.radarr.kodi_metadata, Some(true));
        assert_eq!(first.sonarr.kodi_metadata, Some(false));
        assert_eq!(second.radarr.kodi_metadata, Some(false));
        assert_eq!(second.sonarr.kodi_metadata, Some(true));
    }

    #[tokio::test]
    async fn a_reconcile_records_whether_its_last_call_to_radarr_and_sonarr_succeeded() {
        let library = Library::new().await;
        *library.radarr.lock().unwrap() = Err("401 Unauthorized");
        library.sonarr_lists(&[SEVERANCE]);
        library.failing_episodes.lock().unwrap().insert(SEVERANCE);
        library.reconcile(Source::Radarr).await;
        library.reconcile(Source::Sonarr).await;
        let failed = library.status().await;

        library.radarr_lists(&[]);
        library.failing_episodes.lock().unwrap().clear();
        library.reconcile(Source::Radarr).await;
        library.reconcile(Source::Sonarr).await;
        let succeeded = library.status().await;

        assert!(
            matches!(&failed.radarr.last_api_call, Some(ApiCall::Failed(x)) if x.contains("401 Unauthorized")),
            "{:?}",
            failed.radarr.last_api_call
        );
        assert!(
            matches!(&failed.sonarr.last_api_call, Some(ApiCall::Failed(x)) if x.contains("500 Internal Server Error")),
            "{:?}",
            failed.sonarr.last_api_call
        );
        assert_eq!(succeeded.radarr.last_api_call, Some(ApiCall::Succeeded));
        assert_eq!(succeeded.sonarr.last_api_call, Some(ApiCall::Succeeded));
    }
}
