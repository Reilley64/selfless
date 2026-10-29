use crate::DbContext;
use crate::unsampled;
use crate::webhook_deliveries;
use domain::InboxRow;
use domain::InboxRowStatus;
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
}

#[ohno::error]
#[display("failed to process an inbox row")]
pub struct Error;

#[derive(Deserialize)]
#[serde(tag = "eventType")]
enum RadarrEvent {
    Test,
    Grab,
    Health,
    HealthRestored,
    ApplicationUpdate,
    ManualInteractionRequired,
    #[serde(other)]
    Other,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<bool, Error> {
    let mut db_context = dependencies.into().db_context;
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
        let (status, outcome) = match serde_json::from_slice::<RadarrEvent>(&row.body) {
            Ok(
                RadarrEvent::Test
                | RadarrEvent::Grab
                | RadarrEvent::Health
                | RadarrEvent::HealthRestored
                | RadarrEvent::ApplicationUpdate
                | RadarrEvent::ManualInteractionRequired
                | RadarrEvent::Other,
            ) => (InboxRowStatus::Ignored, "ignored"),
            Err(error) => {
                tracing::warn!(
                    error = &error as &dyn std::error::Error,
                    "failed to read a webhook delivery"
                );
                (InboxRowStatus::Failed, "failed")
            }
        };
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
