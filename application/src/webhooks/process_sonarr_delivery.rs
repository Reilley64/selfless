mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::library::sync_show;
use crate::ports::SonarrWebhook;
use crate::webhooks::webhook_deliveries;
use crate::webhooks::webhook_deliveries::Outcome;
use domain::Source;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    body: Vec<u8>,
) -> Result<Output, Error> {
    let Dependencies {
        db_context,
        get_sonarr_show,
        media_folder,
        read_sonarr_webhook,
        source_health,
    } = dependencies.into();
    let span = Span::current();
    let event_type = webhook_deliveries::event_type(&body);
    span.set_attribute(SELFLESS_WEBHOOK_SOURCE, Source::Sonarr.name());
    if let Some(event_type) = &event_type {
        span.set_attribute(SELFLESS_WEBHOOK_EVENT_TYPE, event_type.clone());
    }
    let synced = match read_sonarr_webhook.call(&body) {
        Ok(SonarrWebhook::Show { sonarr_id }) => sync_show::call(
            sync_show::Dependencies {
                db_context,
                get_sonarr_show,
                media_folder,
                source_health,
            },
            sonarr_id,
        )
        .await
        .map(|()| Outcome::Processed),
        Ok(SonarrWebhook::Ignored) => Ok(Outcome::Ignored),
        Err(error) => {
            tracing::warn!(
                messaging.message.id = %id,
                selfless.webhook.source = Source::Sonarr.name(),
                error = &error as &dyn std::error::Error,
                "skipped a webhook delivery selfless cannot read"
            );
            Ok(Outcome::Skipped)
        }
    };
    let outcome = synced.as_ref().map_or(Outcome::Failed, |x| *x);
    span.set_attribute(SELFLESS_WEBHOOK_OUTCOME, outcome.name());
    webhook_deliveries::count(Source::Sonarr, event_type, outcome);
    synced.map(|_| Output).map_err(Error::caused_by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::SourceHealth;
    use crate::ports::GetSonarrShow;
    use crate::ports::ReadSonarrWebhook;
    use crate::ports::SonarrError;
    use crate::ports::UnreadableWebhook;
    use crate::test::empty_media_folder;
    use crate::test::mock_db_context;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    async fn dependencies(
        read_sonarr_webhook: ReadSonarrWebhook,
        show_calls: Arc<AtomicUsize>,
    ) -> Dependencies {
        Dependencies {
            db_context: mock_db_context().await,
            get_sonarr_show: GetSonarrShow::new(move |_| {
                show_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(SonarrError::caused_by("Sonarr is down")) })
            }),
            media_folder: empty_media_folder(),
            read_sonarr_webhook,
            source_health: SourceHealth::default(),
        }
    }

    #[tokio::test]
    async fn an_unreadable_body_is_skipped() {
        let show_calls = Arc::new(AtomicUsize::new(0));
        let dependencies = dependencies(
            ReadSonarrWebhook::new(|_| Err(UnreadableWebhook::caused_by("missing field `series`"))),
            show_calls.clone(),
        )
        .await;

        let skipped = call(
            dependencies,
            Uuid::nil(),
            br#"{"eventType":"Download"}"#.to_vec(),
        )
        .await;

        assert!(skipped.is_ok());
        assert_eq!(show_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_failed_sync_returns_an_error() {
        let show_calls = Arc::new(AtomicUsize::new(0));
        let dependencies = dependencies(
            ReadSonarrWebhook::new(|_| Ok(SonarrWebhook::Show { sonarr_id: 42 })),
            show_calls.clone(),
        )
        .await;

        let failed = call(
            dependencies,
            Uuid::nil(),
            br#"{"eventType":"Download"}"#.to_vec(),
        )
        .await;

        assert!(failed.is_err());
        assert_eq!(show_calls.load(Ordering::Relaxed), 1);
    }
}
