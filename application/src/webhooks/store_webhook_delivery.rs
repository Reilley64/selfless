mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::source_states;
use crate::ports::persistence::DbScope;
use crate::webhooks::webhook_deliveries;
use crate::webhooks::webhook_deliveries::Outcome;
use bytes::Bytes;
use domain::Source;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    source: Source,
    password: Option<String>,
    body: Bytes,
) -> Result<Output, Error> {
    let Dependencies {
        clock,
        mut db_context,
        read_webhook_event_type,
        webhook_passwords,
    } = dependencies.into();
    if password.as_ref() != Some(webhook_passwords.get(source)) {
        source_states::upsert(&mut db_context, source)
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
        return Ok(Output::Rejected);
    }
    let event_type = read_webhook_event_type.call(&body);
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
    source_states::upsert(&mut db_context, source)
        .await
        .map_err(Error::caused_by)?
        .update()
        .last_webhook_at(Some(received_at))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    webhook_deliveries::count(source, event_type, Outcome::Stored);
    Ok(Output::Stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::ReadWebhookEventType;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use crate::webhooks::WebhookPasswords;
    use domain::SourceState;
    use jiff::Timestamp;
    use test_fixtures::FakeClock;

    const RADARR_PASSWORD: &str = "radarr webhook password";
    const SONARR_PASSWORD: &str = "sonarr webhook password";
    const BODY: &[u8] = br#"{"eventType":"Test"}"#;

    async fn store(
        db_context: &DbContext,
        clock: &FakeClock,
        source: Source,
        password: Option<&str>,
    ) -> Output {
        call(
            Dependencies {
                clock: Clock::new(clock.reader()),
                db_context: db_context.clone(),
                read_webhook_event_type: ReadWebhookEventType::new(|_| None),
                webhook_passwords: WebhookPasswords {
                    radarr: RADARR_PASSWORD.to_owned(),
                    sonarr: SONARR_PASSWORD.to_owned(),
                },
            },
            source,
            password.map(str::to_owned),
            Bytes::from_static(BODY),
        )
        .await
        .unwrap()
    }

    async fn last_webhook_at(db_context: &mut DbContext, source: Source) -> Option<Timestamp> {
        db_context
            .source_states()
            .filter(SourceState::fields().source().eq(source))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .and_then(|x| x.last_webhook_at)
    }

    async fn inbox(db_context: &mut DbContext) -> Vec<(Source, Vec<u8>)> {
        db_context
            .inboxes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.source, x.body))
            .collect()
    }

    #[tokio::test]
    async fn a_stored_delivery_is_in_the_inbox_and_is_the_last_webhook_of_its_source_only() {
        for (source, password, other) in [
            (Source::Radarr, RADARR_PASSWORD, Source::Sonarr),
            (Source::Sonarr, SONARR_PASSWORD, Source::Radarr),
        ] {
            let mut db_context = mock_db_context().await;
            let clock = FakeClock::new();

            let stored = store(&db_context, &clock, source, Some(password)).await;

            assert!(matches!(stored, Output::Stored), "{source:?}");
            assert_eq!(inbox(&mut db_context).await, [(source, BODY.to_vec())]);
            assert_eq!(
                last_webhook_at(&mut db_context, source).await,
                Some(clock.now())
            );
            assert_eq!(last_webhook_at(&mut db_context, other).await, None);
        }
    }

    #[tokio::test]
    async fn a_wrong_or_missing_password_is_rejected_and_stores_nothing() {
        for password in [Some("wrong password"), None] {
            let mut db_context = mock_db_context().await;
            let clock = FakeClock::new();

            let rejected = store(&db_context, &clock, Source::Radarr, password).await;

            assert!(matches!(rejected, Output::Rejected), "{password:?}");
            assert_eq!(inbox(&mut db_context).await, []);
            assert_eq!(last_webhook_at(&mut db_context, Source::Radarr).await, None);
        }
    }

    #[tokio::test]
    async fn the_radarr_password_is_rejected_for_sonarr() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();

        let rejected = store(&db_context, &clock, Source::Sonarr, Some(RADARR_PASSWORD)).await;

        assert!(matches!(rejected, Output::Rejected));
        assert_eq!(inbox(&mut db_context).await, []);
    }
}
