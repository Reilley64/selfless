use crate::DbContext;
use crate::Reconciles;
use crate::attributes::SELFLESS_LIBRARY_SOURCE;
use crate::attributes::SELFLESS_RECONCILE_RESULT;
use crate::metrics::SELFLESS_RECONCILE_DURATION;
use crate::ports::Clock;
use crate::ports::GetSonarrEpisodes;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrSeries;
use crate::ports::SonarrShow;
use crate::sync_movie;
use crate::sync_show;
use crate::sync_status::ReconcileProgress;
use domain::HeldRemoval;
use domain::InboxRow;
use domain::InboxRowStatus;
use domain::Movie;
use domain::ReconcileResult;
use domain::Show;
use domain::Source;
use opentelemetry::KeyValue;
use opentelemetry::global;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;
use tracing::Instrument;
use tracing::field;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub get_sonarr_episodes: GetSonarrEpisodes,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_series: ListSonarrSeries,
    pub reconciles: Reconciles,
}

#[ohno::error]
#[display("failed to reconcile the library")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<(), Error> {
    let Dependencies {
        clock,
        mut db_context,
        get_sonarr_episodes,
        list_radarr_movies,
        list_sonarr_series,
        reconciles,
    } = dependencies.into();
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
            reconcile_movies(&mut db_context, &list_radarr_movies, progress)
                .instrument(span.clone())
                .await
        }
        Source::Sonarr => {
            reconcile_shows(
                &mut db_context,
                &list_sonarr_series,
                &get_sonarr_episodes,
                progress,
            )
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

async fn reconcile_movies(
    db_context: &mut DbContext,
    list_radarr_movies: &ListRadarrMovies,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<(), Error> {
    let radarr_movies = list_radarr_movies.call().await.map_err(Error::caused_by)?;
    let listed = radarr_movies
        .iter()
        .map(|x| x.tmdb_id)
        .collect::<HashSet<_>>();
    set_total(progress, radarr_movies.len());
    let mut synced = HashSet::new();
    for radarr_movie in radarr_movies {
        synced.insert(radarr_movie.radarr_id);
        sync_movie::save(db_context, radarr_movie)
            .instrument(sync_movie::span())
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
    list_sonarr_series: &ListSonarrSeries,
    get_sonarr_episodes: &GetSonarrEpisodes,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<(), Error> {
    let series = list_sonarr_series.call().await.map_err(Error::caused_by)?;
    let listed = series.iter().map(|x| x.tvdb_id).collect::<HashSet<_>>();
    set_total(progress, series.len());
    let mut failed_calls = 0;
    let mut synced = HashSet::new();
    for series in series {
        let span = sync_show::span();
        match get_sonarr_episodes
            .call(series.sonarr_id)
            .instrument(span.clone())
            .await
        {
            Ok(episodes) => {
                synced.insert(series.sonarr_id);
                sync_show::save(db_context, SonarrShow { series, episodes })
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
