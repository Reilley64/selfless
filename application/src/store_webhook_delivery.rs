use crate::DbContext;
use crate::InboxWakeup;
use crate::WebhookPasswords;
use crate::ports::Clock;
use crate::webhook_deliveries;
use bytes::Bytes;
use domain::InboxRow;
use domain::InboxRowStatus;
use domain::Source;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub inbox_wakeup: InboxWakeup,
    pub webhook_passwords: WebhookPasswords,
}

#[ohno::error]
#[display("failed to store a webhook delivery")]
pub struct Error;

pub enum Delivery {
    Stored,
    Rejected,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventType {
    event_type: String,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    source: Source,
    password: Option<String>,
    body: Bytes,
) -> Result<Delivery, Error> {
    let Dependencies {
        clock,
        mut db_context,
        inbox_wakeup,
        webhook_passwords,
    } = dependencies.into();
    let expected_password = match source {
        Source::Radarr => webhook_passwords.radarr,
        Source::Sonarr => webhook_passwords.sonarr,
    };
    if password != Some(expected_password) {
        toasty::sql::statement(
            "INSERT INTO source_states (source, rejected_webhooks) VALUES ($1, 1) \
             ON CONFLICT (source) DO UPDATE SET rejected_webhooks = source_states.rejected_webhooks + 1",
        )
        .bind(source.name())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
        webhook_deliveries::count(source, None, "rejected");
        tracing::warn!(
            selfless.webhook.source = source.name(),
            "rejected a webhook delivery with a wrong or missing password"
        );
        return Ok(Delivery::Rejected);
    }
    let event_type = serde_json::from_slice::<EventType>(&body)
        .ok()
        .map(|x| x.event_type);
    let received_at = clock.call();
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&Span::current().context(), &mut carrier);
    toasty::create!(InboxRow {
        source,
        event_type: event_type.clone(),
        body: body.to_vec(),
        received_at,
        traceparent: carrier.remove("traceparent"),
        status: InboxRowStatus::Pending,
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    inbox_wakeup.wake();
    toasty::sql::statement(
        "INSERT INTO source_states (source, last_webhook_at, rejected_webhooks) VALUES ($1, $2, 0) \
         ON CONFLICT (source) DO UPDATE SET last_webhook_at = EXCLUDED.last_webhook_at",
    )
    .bind(source.name())
    .bind(received_at)
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    webhook_deliveries::count(source, event_type, "stored");
    Ok(Delivery::Stored)
}
