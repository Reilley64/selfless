use crate::CONCURRENT_PROBES;
use crate::DbContext;
use crate::Prober;
use crate::Reconciles;
use crate::SourceHealth;
use crate::attributes::SELFLESS_LIBRARY_SOURCE;
use crate::attributes::SELFLESS_RECONCILE_RESULT;
use crate::media_files::Inspector;
use crate::metrics::SELFLESS_RECONCILE_DURATION;
use crate::ports::CheckRadarrKodiMetadata;
use crate::ports::CheckSonarrKodiMetadata;
use crate::ports::Clock;
use crate::ports::GetSonarrEpisodes;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrSeries;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::SonarrShow;
use crate::sync_movie;
use crate::sync_show;
use crate::sync_status::ReconcileProgress;
use domain::HeldRemoval;
use domain::InboxRow;
use domain::InboxRowStatus;
use domain::MediaFile;
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

#[fundle::deps]
pub struct Dependencies {
    pub check_radarr_kodi_metadata: CheckRadarrKodiMetadata,
    pub check_sonarr_kodi_metadata: CheckSonarrKodiMetadata,
    pub clock: Clock,
    pub db_context: DbContext,
    pub get_sonarr_episodes: GetSonarrEpisodes,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_series: ListSonarrSeries,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to reconcile the library")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<(), Error> {
    let Dependencies {
        check_radarr_kodi_metadata,
        check_sonarr_kodi_metadata,
        clock,
        mut db_context,
        get_sonarr_episodes,
        list_radarr_movies,
        list_sonarr_series,
        list_subtitle_files,
        prober,
        read_movie_folder,
        read_movie_nfo,
        reconciles,
        source_health,
    } = dependencies.into();
    let inspector = Inspector {
        prober,
        list_subtitle_files,
    };
    let reconcile = reconciles.source(source);
    let Ok(_running) = reconcile.running.clone().try_lock_owned() else {
        return Ok(());
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
    let reconciled = match source {
        Source::Radarr => {
            async {
                let kodi_metadata = check_radarr_kodi_metadata.call().await;
                record_kodi_metadata(&source_health, source, kodi_metadata);
                reconcile_movies(
                    &mut db_context,
                    &inspector,
                    &list_radarr_movies,
                    &read_movie_folder,
                    &read_movie_nfo,
                    &source_health,
                    progress,
                )
                .await
            }
            .instrument(span.clone())
            .await
        }
        Source::Sonarr => {
            async {
                let kodi_metadata = check_sonarr_kodi_metadata.call().await;
                record_kodi_metadata(&source_health, source, kodi_metadata);
                reconcile_shows(
                    &mut db_context,
                    &inspector,
                    &list_sonarr_series,
                    &get_sonarr_episodes,
                    &source_health,
                    progress,
                )
                .await
            }
            .instrument(span.clone())
            .await
        }
    };
    *progress.lock().unwrap() = None;
    let duration = start.elapsed();
    let result = match reconciled {
        Ok(()) => ("succeeded", ReconcileResult::Succeeded),
        Err(error) => {
            span.in_scope(|| {
                tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    selfless.library.source = source.name(),
                    "failed to reconcile the library"
                );
            });
            ("failed", ReconcileResult::Failed)
        }
    };
    span.record("selfless.reconcile.result", result.0);
    global::meter("selfless")
        .f64_histogram(SELFLESS_RECONCILE_DURATION)
        .with_unit("s")
        .build()
        .record(
            duration.as_secs_f64(),
            &[
                KeyValue::new(SELFLESS_LIBRARY_SOURCE, source.name()),
                KeyValue::new(SELFLESS_RECONCILE_RESULT, result.0),
            ],
        );
    toasty::sql::statement(
        "INSERT INTO source_states (source, rejected_webhooks, last_reconcile_started_at, \
         last_reconcile_duration_ms, last_reconcile_result) VALUES ($1, 0, $2, $3, $4) \
         ON CONFLICT (source) DO UPDATE SET \
         last_reconcile_started_at = EXCLUDED.last_reconcile_started_at, \
         last_reconcile_duration_ms = EXCLUDED.last_reconcile_duration_ms, \
         last_reconcile_result = EXCLUDED.last_reconcile_result",
    )
    .bind(source.name())
    .bind(started_at)
    .bind(i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
    .bind(result.0)
    .exec(db_context.executor_mut())
    .instrument(span)
    .await
    .map_err(Error::caused_by)?;
    Ok(())
}

fn record_kodi_metadata(
    source_health: &SourceHealth,
    source: Source,
    kodi_metadata: Result<bool, impl std::error::Error + 'static>,
) {
    match source_health.api_call(source, kodi_metadata) {
        Ok(enabled) => source_health.source(source).lock().unwrap().kodi_metadata = Some(enabled),
        Err(error) => tracing::warn!(
            error = &error as &dyn std::error::Error,
            selfless.library.source = source.name(),
            "failed to read the metadata settings"
        ),
    }
}

async fn reconcile_movies(
    db_context: &mut DbContext,
    inspector: &Inspector,
    list_radarr_movies: &ListRadarrMovies,
    read_movie_folder: &ReadMovieFolder,
    read_movie_nfo: &ReadMovieNfo,
    source_health: &SourceHealth,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<(), Error> {
    let radarr_movies = source_health
        .api_call(Source::Radarr, list_radarr_movies.call().await)
        .map_err(Error::caused_by)?;
    let listed = radarr_movies
        .iter()
        .map(|x| x.tmdb_id)
        .collect::<HashSet<_>>();
    set_total(progress, radarr_movies.len());
    let synced = radarr_movies
        .iter()
        .map(|x| x.radarr_id)
        .collect::<HashSet<_>>();
    let stored = MediaFile::all()
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
                let inspected = sync_movie::inspect(inspector, stored, &radarr_movie)
                    .instrument(span.clone())
                    .await;
                (radarr_movie, inspected, span)
            }
        })
        .buffered(CONCURRENT_PROBES);
    while let Some((radarr_movie, inspected, span)) = inspections.next().await {
        let inspected = inspected.map_err(Error::caused_by)?;
        sync_movie::save(
            db_context,
            read_movie_folder,
            read_movie_nfo,
            radarr_movie,
            inspected,
        )
        .instrument(span)
        .await
        .map_err(Error::caused_by)?;
        imported(progress);
    }
    resolve_failed_rows(db_context, Source::Radarr, &synced).await?;
    let movies = Movie::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let total = movies.len();
    let removed = movies
        .into_iter()
        .filter(|x| !listed.contains(&x.tmdb_id))
        .map(|x| x.id)
        .collect();
    remove_or_hold(db_context, Source::Radarr, total, removed).await
}

async fn reconcile_shows(
    db_context: &mut DbContext,
    inspector: &Inspector,
    list_sonarr_series: &ListSonarrSeries,
    get_sonarr_episodes: &GetSonarrEpisodes,
    source_health: &SourceHealth,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<(), Error> {
    let series = source_health
        .api_call(Source::Sonarr, list_sonarr_series.call().await)
        .map_err(Error::caused_by)?;
    let listed = series.iter().map(|x| x.tvdb_id).collect::<HashSet<_>>();
    set_total(progress, series.len());
    let mut failed_calls = 0;
    let mut synced = HashSet::new();
    for series in series {
        let span = sync_show::span();
        let episodes = get_sonarr_episodes
            .call(series.sonarr_id)
            .instrument(span.clone())
            .await;
        match source_health.api_call(Source::Sonarr, episodes) {
            Ok(episodes) => {
                synced.insert(series.sonarr_id);
                sync_show::save(db_context, inspector, SonarrShow { series, episodes })
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
    resolve_failed_rows(db_context, Source::Sonarr, &synced).await?;
    if failed_calls > 0 {
        return Err(Error::caused_by(format!(
            "{failed_calls} calls to Sonarr failed, so nothing was removed"
        )));
    }
    let shows = Show::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let total = shows.len();
    let removed = shows
        .into_iter()
        .filter(|x| !listed.contains(&x.tvdb_id))
        .map(|x| x.id)
        .collect();
    remove_or_hold(db_context, Source::Sonarr, total, removed).await
}

async fn resolve_failed_rows(
    db_context: &mut DbContext,
    source: Source,
    synced: &HashSet<i64>,
) -> Result<(), Error> {
    let failed_rows = InboxRow::filter(
        InboxRow::fields()
            .status()
            .eq(InboxRowStatus::Failed)
            .and(InboxRow::fields().source().eq(source)),
    )
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    for mut row in failed_rows {
        if row.source_item_id.is_some_and(|x| synced.contains(&x)) {
            row.update()
                .status(InboxRowStatus::Resolved)
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
    }
    Ok(())
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
    total: usize,
    removed: Vec<Uuid>,
) -> Result<(), Error> {
    HeldRemoval::filter_by_source(source)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if removed.len() * 2 > total {
        tracing::warn!(
            selfless.library.source = source.name(),
            "held a removal of more than half of a source's items"
        );
        toasty::create!(HeldRemoval {
            source,
            item_ids: removed,
        })
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
        return Ok(());
    }
    remove(db_context, source, removed).await
}

pub(crate) async fn remove(
    db_context: &mut DbContext,
    source: Source,
    ids: Vec<Uuid>,
) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    match source {
        Source::Radarr => {
            let movies = Movie::filter(Movie::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            sync_movie::remove(db_context, movies)
                .await
                .map_err(Error::caused_by)
        }
        Source::Sonarr => {
            let shows = Show::filter(Show::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            for show in shows {
                sync_show::remove(db_context, show)
                    .await
                    .map_err(Error::caused_by)?;
            }
            Ok(())
        }
    }
}
