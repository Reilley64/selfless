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
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::SonarrShow;
use crate::process_inbox_row;
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
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
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
        read_show_folder,
        read_show_nfo,
        read_episode_folder,
        read_episode_nfo,
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
    let reconciled = async {
        let failed_metadata_calls = match source {
            Source::Radarr => record_kodi_metadata(
                &source_health,
                source,
                check_radarr_kodi_metadata.call().await,
            ),
            Source::Sonarr => record_kodi_metadata(
                &source_health,
                source,
                check_sonarr_kodi_metadata.call().await,
            ),
        };
        let listing = match source {
            Source::Radarr => {
                sync_movies(
                    &mut db_context,
                    &inspector,
                    &list_radarr_movies,
                    &read_movie_folder,
                    &read_movie_nfo,
                    &source_health,
                    progress,
                )
                .await?
            }
            Source::Sonarr => {
                sync_shows(
                    &mut db_context,
                    &inspector,
                    &list_sonarr_series,
                    &get_sonarr_episodes,
                    sync_show::Readers {
                        read_show_folder: &read_show_folder,
                        read_show_nfo: &read_show_nfo,
                        read_episode_folder: &read_episode_folder,
                        read_episode_nfo: &read_episode_nfo,
                    },
                    &source_health,
                    progress,
                )
                .await?
            }
        };
        let failed_calls = failed_metadata_calls + listing.failed_calls;
        if failed_calls == 0 {
            remove_or_hold(&mut db_context, source, &listing).await?;
        }
        resolve_failed_rows(&mut db_context, source, &listing.synced).await?;
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
) -> usize {
    match source_health.api_call(source, kodi_metadata) {
        Ok(enabled) => {
            source_health.source(source).lock().unwrap().kodi_metadata = Some(enabled);
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
    synced: HashSet<i64>,
    failed_calls: usize,
}

async fn sync_movies(
    db_context: &mut DbContext,
    inspector: &Inspector,
    list_radarr_movies: &ListRadarrMovies,
    read_movie_folder: &ReadMovieFolder,
    read_movie_nfo: &ReadMovieNfo,
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
    let synced = radarr_movies
        .iter()
        .map(|x| x.radarr_id)
        .collect::<HashSet<_>>();
    let stored_movies = Movie::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let total = stored_movies.len();
    let absent = stored_movies
        .into_iter()
        .filter(|x| !listed.contains(&x.tmdb_id))
        .map(|x| x.id)
        .collect();
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
    Ok(Listing {
        listed,
        total,
        absent,
        synced,
        failed_calls: 0,
    })
}

async fn sync_shows(
    db_context: &mut DbContext,
    inspector: &Inspector,
    list_sonarr_series: &ListSonarrSeries,
    get_sonarr_episodes: &GetSonarrEpisodes,
    readers: sync_show::Readers<'_>,
    source_health: &SourceHealth,
    progress: &Mutex<Option<ReconcileProgress>>,
) -> Result<Listing, Error> {
    let series = source_health
        .api_call(Source::Sonarr, list_sonarr_series.call().await)
        .map_err(Error::caused_by)?;
    let listed = series.iter().map(|x| x.tvdb_id).collect::<HashSet<_>>();
    set_total(progress, series.len());
    let stored_shows = Show::all()
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
                sync_show::save(
                    db_context,
                    inspector,
                    readers,
                    SonarrShow { series, episodes },
                )
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
        synced,
        failed_calls,
    })
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
    let stored = match source {
        Source::Radarr => Movie::all()
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.radarr_id)
            .collect::<HashSet<_>>(),
        Source::Sonarr => Show::all()
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.sonarr_id)
            .collect(),
    };
    for mut row in failed_rows {
        let Some(item_id) = row.source_item_id else {
            continue;
        };
        let gone = process_inbox_row::is_removal(source, row.event_type.as_deref())
            && !stored.contains(&item_id);
        if synced.contains(&item_id) || gone {
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
    listing: &Listing,
) -> Result<(), Error> {
    HeldRemoval::filter_by_source(source)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if listing.absent.len() * 2 > listing.total {
        tracing::warn!(
            selfless.library.source = source.name(),
            "held a removal of more than half of a source's items"
        );
        toasty::create!(HeldRemoval {
            source,
            item_ids: listing.absent.clone(),
        })
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
            let movies = Movie::filter(Movie::fields().id().in_list(ids))
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
            let shows = Show::filter(Show::fields().id().in_list(ids))
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
