use crate::DbContext;
use crate::ports::GetRadarrMovie;
use crate::ports::GetSonarrShow;
use crate::sync_movie;
use crate::sync_show;
use crate::unsampled;
use crate::webhook_deliveries;
use domain::InboxRow;
use domain::InboxRowStatus;
use domain::Source;
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
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
    pub get_sonarr_show: GetSonarrShow,
}

#[ohno::error]
#[display("failed to process an inbox row")]
pub struct Error;

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

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<bool, Error> {
    let Dependencies {
        mut db_context,
        get_radarr_movie,
        get_sonarr_show,
    } = dependencies.into();
    let Some(claimed) = toasty::sql::query(
        "UPDATE inbox_rows SET status = 'processing' WHERE id = (\
         SELECT id FROM inbox_rows WHERE status = 'pending' \
         ORDER BY received_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, traceparent",
    )
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
        span.record("selfless.webhook.source", row.source.name());
        span.record("selfless.webhook.event_type", row.event_type.as_deref());
        let sync_movie = sync_movie::Dependencies {
            db_context: db_context.clone(),
            get_radarr_movie,
        };
        let sync_show = sync_show::Dependencies {
            db_context: db_context.clone(),
            get_sonarr_show,
        };
        let (status, outcome) = dispatch(row.source, &row.body, sync_movie, sync_show)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(
                    error = &*error as &dyn std::error::Error,
                    "failed to process a webhook delivery"
                );
                (InboxRowStatus::Failed, "failed")
            });
        span.record("selfless.webhook.outcome", outcome);
        let (source, event_type) = (row.source, row.event_type.clone());
        row.update()
            .status(status)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        webhook_deliveries::count(source, event_type, outcome);
        Ok(true)
    }
    .instrument(span)
    .await
}

async fn dispatch(
    source: Source,
    body: &[u8],
    sync_movie: sync_movie::Dependencies,
    sync_show: sync_show::Dependencies,
) -> Result<(InboxRowStatus, &'static str), Box<dyn std::error::Error + Send + Sync>> {
    let processed = (InboxRowStatus::Processed, "processed");
    let ignored = (InboxRowStatus::Ignored, "ignored");
    match source {
        Source::Radarr => match serde_json::from_slice(body)? {
            RadarrEvent::Download { movie }
            | RadarrEvent::Rename { movie }
            | RadarrEvent::MovieAdded { movie }
            | RadarrEvent::MovieDelete { movie }
            | RadarrEvent::MovieFileDelete { movie } => {
                sync_movie::call(sync_movie, movie.id).await?;
                Ok(processed)
            }
            RadarrEvent::Test
            | RadarrEvent::Grab
            | RadarrEvent::Health
            | RadarrEvent::HealthRestored
            | RadarrEvent::ApplicationUpdate
            | RadarrEvent::ManualInteractionRequired
            | RadarrEvent::Other => Ok(ignored),
        },
        Source::Sonarr => match serde_json::from_slice(body)? {
            SonarrEvent::Download { series }
            | SonarrEvent::Rename { series }
            | SonarrEvent::SeriesAdd { series }
            | SonarrEvent::SeriesDelete { series }
            | SonarrEvent::EpisodeFileDelete { series } => {
                sync_show::call(sync_show, series.id).await?;
                Ok(processed)
            }
            SonarrEvent::Test
            | SonarrEvent::Grab
            | SonarrEvent::Health
            | SonarrEvent::HealthRestored
            | SonarrEvent::ApplicationUpdate
            | SonarrEvent::ManualInteractionRequired
            | SonarrEvent::Other => Ok(ignored),
        },
    }
}
