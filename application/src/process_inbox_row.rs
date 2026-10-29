use crate::DbContext;
use crate::Prober;
use crate::SourceHealth;
use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::ports::Clock;
use crate::ports::GetRadarrMovie;
use crate::ports::GetSonarrShow;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::sync_movie;
use crate::sync_show;
use crate::unsampled;
use crate::webhook_deliveries;
use domain::InboxRow;
use domain::InboxRowStatus;
use domain::Source;
use jiff::SignedDuration;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::Instrument;
use tracing::Span;
use tracing::field;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
    pub get_sonarr_show: GetSonarrShow,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to process an inbox row")]
pub struct Error;

const RETRY_DELAYS: [SignedDuration; 4] = [
    SignedDuration::from_mins(1),
    SignedDuration::from_mins(5),
    SignedDuration::from_mins(15),
    SignedDuration::from_mins(30),
];

#[derive(Deserialize)]
#[serde(tag = "eventType")]
enum RadarrEvent {
    Download {
        movie: RadarrMovieRef,
    },
    Rename {
        movie: RadarrMovieRef,
    },
    MovieAdded {
        movie: RadarrMovieRef,
    },
    MovieDelete {
        movie: RadarrMovieRef,
    },
    MovieFileDelete {
        movie: RadarrMovieRef,
    },
    Test,
    Grab,
    Health,
    HealthRestored,
    ApplicationUpdate,
    ManualInteractionRequired,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "eventType")]
enum SonarrEvent {
    Download {
        series: SonarrSeries,
    },
    Rename {
        series: SonarrSeries,
    },
    SeriesAdd {
        series: SonarrSeries,
    },
    SeriesDelete {
        series: SonarrSeries,
    },
    EpisodeFileDelete {
        series: SonarrSeries,
    },
    Test,
    Grab,
    Health,
    HealthRestored,
    ApplicationUpdate,
    ManualInteractionRequired,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct RadarrMovieRef {
    id: i64,
}

#[derive(Deserialize)]
struct SonarrSeries {
    id: i64,
}

enum Target {
    Movie(i64),
    Show(i64),
    Nothing,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<bool, Error> {
    let Dependencies {
        clock,
        mut db_context,
        get_radarr_movie,
        get_sonarr_show,
        list_subtitle_files,
        prober,
        read_movie_folder,
        read_movie_nfo,
        read_show_folder,
        read_show_nfo,
        read_episode_folder,
        read_episode_nfo,
        source_health,
    } = dependencies.into();
    let claimed_at = clock.call();
    let Some(claimed) = toasty::sql::query(
        "UPDATE inbox_rows SET status = 'processing', attempts = attempts + 1 WHERE id = (\
         SELECT id FROM inbox_rows WHERE status = 'pending' AND next_attempt_at <= $1 \
         ORDER BY received_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, traceparent",
    )
    .bind(claimed_at)
    .exec(db_context.executor_mut())
    .instrument(unsampled::inbox_poll())
    .await
    .map_err(Error::caused_by)?
    .pop() else {
        return Ok(false);
    };
    let mut claimed = claimed.into_record().fields;
    let traceparent = String::try_from(claimed.remove(1)).ok();
    let id = Uuid::try_from(claimed.remove(0)).map_err(Error::caused_by)?;
    let span = tracing::info_span!(
        parent: None,
        "process_webhook_delivery",
        otel.name = field::Empty,
        otel.kind = "consumer",
        selfless.webhook.source = field::Empty,
        selfless.webhook.event_type = field::Empty,
        selfless.webhook.outcome = field::Empty,
    );
    if let Some(traceparent) = traceparent {
        let carrier = HashMap::from([("traceparent".to_owned(), traceparent)]);
        span.add_link(
            TraceContextPropagator::new()
                .extract(&carrier)
                .span()
                .span_context()
                .clone(),
        );
    }
    async {
        let mut row = InboxRow::get_by_id(db_context.executor_mut(), &id)
            .await
            .map_err(Error::caused_by)?;
        let span = Span::current();
        span.record(
            "otel.name",
            format!("process {} webhook", row.source.name()),
        );
        span.record(SELFLESS_WEBHOOK_SOURCE, row.source.name());
        span.record(SELFLESS_WEBHOOK_EVENT_TYPE, row.event_type.as_deref());
        let target = target(row.source, &row.body);
        let source_item_id = match target {
            Ok(Target::Movie(id) | Target::Show(id)) => Some(id),
            Ok(Target::Nothing) | Err(_) => None,
        };
        let processed: Result<_, Box<dyn std::error::Error + Send + Sync>> = match target {
            Ok(Target::Movie(radarr_id)) => sync_movie::call(
                sync_movie::Dependencies {
                    db_context: db_context.clone(),
                    get_radarr_movie,
                    list_subtitle_files,
                    prober,
                    read_movie_folder,
                    read_movie_nfo,
                    source_health,
                },
                radarr_id,
            )
            .await
            .map(|()| (InboxRowStatus::Processed, "processed"))
            .map_err(Into::into),
            Ok(Target::Show(sonarr_id)) => sync_show::call(
                sync_show::Dependencies {
                    db_context: db_context.clone(),
                    get_sonarr_show,
                    read_episode_folder,
                    read_episode_nfo,
                    read_show_folder,
                    read_show_nfo,
                    source_health,
                    list_subtitle_files,
                    prober,
                },
                sonarr_id,
            )
            .await
            .map(|()| (InboxRowStatus::Processed, "processed"))
            .map_err(Into::into),
            Ok(Target::Nothing) => Ok((InboxRowStatus::Ignored, "ignored")),
            Err(error) => Err(error.into()),
        };
        let (status, next_attempt_at, error, outcome) = match processed {
            Ok((status, outcome)) => (status, row.next_attempt_at, None, outcome),
            Err(error) => {
                tracing::warn!(
                    error = &*error as &dyn std::error::Error,
                    "failed to process a webhook delivery"
                );
                let retry = usize::try_from(row.attempts - 1)
                    .ok()
                    .and_then(|x| RETRY_DELAYS.get(x));
                let (status, next_attempt_at) = match retry {
                    Some(delay) => (InboxRowStatus::Pending, claimed_at + *delay),
                    None => (InboxRowStatus::Failed, row.next_attempt_at),
                };
                (status, next_attempt_at, Some(error.to_string()), "failed")
            }
        };
        span.record(SELFLESS_WEBHOOK_OUTCOME, outcome);
        let (source, event_type) = (row.source, row.event_type.clone());
        row.update()
            .status(status)
            .next_attempt_at(next_attempt_at)
            .error(error)
            .source_item_id(source_item_id)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        if status != InboxRowStatus::Pending {
            webhook_deliveries::count(source, event_type, outcome);
        }
        Ok(true)
    }
    .instrument(span)
    .await
}

pub(crate) fn is_removal(source: Source, event_type: Option<&str>) -> bool {
    matches!(
        (source, event_type),
        (Source::Radarr, Some("MovieDelete")) | (Source::Sonarr, Some("SeriesDelete"))
    )
}

fn target(source: Source, body: &[u8]) -> Result<Target, serde_json::Error> {
    Ok(match source {
        Source::Radarr => match serde_json::from_slice(body)? {
            RadarrEvent::Download { movie }
            | RadarrEvent::Rename { movie }
            | RadarrEvent::MovieAdded { movie }
            | RadarrEvent::MovieDelete { movie }
            | RadarrEvent::MovieFileDelete { movie } => Target::Movie(movie.id),
            RadarrEvent::Test
            | RadarrEvent::Grab
            | RadarrEvent::Health
            | RadarrEvent::HealthRestored
            | RadarrEvent::ApplicationUpdate
            | RadarrEvent::ManualInteractionRequired
            | RadarrEvent::Other => Target::Nothing,
        },
        Source::Sonarr => match serde_json::from_slice(body)? {
            SonarrEvent::Download { series }
            | SonarrEvent::Rename { series }
            | SonarrEvent::SeriesAdd { series }
            | SonarrEvent::SeriesDelete { series }
            | SonarrEvent::EpisodeFileDelete { series } => Target::Show(series.id),
            SonarrEvent::Test
            | SonarrEvent::Grab
            | SonarrEvent::Health
            | SonarrEvent::HealthRestored
            | SonarrEvent::ApplicationUpdate
            | SonarrEvent::ManualInteractionRequired
            | SonarrEvent::Other => Target::Nothing,
        },
    })
}
