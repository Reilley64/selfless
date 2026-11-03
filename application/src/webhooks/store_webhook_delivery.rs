use crate::library::source_states;
use crate::ports::Clock;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use crate::webhooks::WebhookPasswords;
use crate::webhooks::webhook_deliveries;
use crate::webhooks::webhook_deliveries::Outcome;
use bytes::Bytes;
use domain::Source;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub webhook_passwords: WebhookPasswords,
}

#[ohno::error]
#[display("failed to store a webhook delivery")]
pub struct Error;

pub enum Delivery {
    Stored,
    Rejected,
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
        webhook_passwords,
    } = dependencies.into();
    if password.as_ref() != Some(webhook_passwords.get(source)) {
        source_states::upsert(db_context.executor_mut(), source)
            .await
            .map_err(Error::caused_by)?;
        toasty::sql::statement(
            "UPDATE source_states SET rejected_webhooks = rejected_webhooks + 1 WHERE source = $1",
        )
        .bind(source.name())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
        webhook_deliveries::count(source, None, Outcome::Rejected);
        tracing::warn!(
            selfless.webhook.source = source.name(),
            "rejected a webhook delivery with a wrong or missing password"
        );
        return Ok(Delivery::Rejected);
    }
    let event_type = webhook_deliveries::event_type(&body);
    let received_at = clock.call();
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&Span::current().context(), &mut carrier);
    db_context
        .inboxes()
        .create()
        .source(source)
        .body(body.to_vec())
        .received_at(received_at)
        .traceparent(carrier.remove("traceparent"))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    source_states::upsert(db_context.executor_mut(), source)
        .await
        .map_err(Error::caused_by)?
        .update()
        .last_webhook_at(Some(received_at))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    webhook_deliveries::count(source, event_type, Outcome::Stored);
    Ok(Delivery::Stored)
}
