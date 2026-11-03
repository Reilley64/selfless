mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::ports::RadarrWebhook;
use crate::sync_movie;
use crate::webhook_deliveries;
use crate::webhook_deliveries::Outcome;
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
        get_radarr_movie,
        media_folder,
        read_radarr_webhook,
        source_health,
    } = dependencies.into();
    let span = Span::current();
    let event_type = webhook_deliveries::event_type(&body);
    span.set_attribute(SELFLESS_WEBHOOK_SOURCE, Source::Radarr.name());
    if let Some(event_type) = &event_type {
        span.set_attribute(SELFLESS_WEBHOOK_EVENT_TYPE, event_type.clone());
    }
    let synced = match read_radarr_webhook.call(&body) {
        Ok(RadarrWebhook::Movie { radarr_id }) => sync_movie::call(
            sync_movie::Dependencies {
                db_context,
                get_radarr_movie,
                media_folder,
                source_health,
            },
            radarr_id,
        )
        .await
        .map(|()| Outcome::Processed),
        Ok(RadarrWebhook::Ignored) => Ok(Outcome::Ignored),
        Err(error) => {
            tracing::warn!(
                messaging.message.id = %id,
                selfless.webhook.source = Source::Radarr.name(),
                error = &error as &dyn std::error::Error,
                "skipped a webhook delivery selfless cannot read"
            );
            Ok(Outcome::Skipped)
        }
    };
    let outcome = synced.as_ref().map_or(Outcome::Failed, |x| *x);
    span.set_attribute(SELFLESS_WEBHOOK_OUTCOME, outcome.name());
    webhook_deliveries::count(Source::Radarr, event_type, outcome);
    synced.map(|_| Output).map_err(Error::caused_by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceHealth;
    use crate::ports::GetRadarrMovie;
    use crate::ports::RadarrError;
    use crate::ports::ReadRadarrWebhook;
    use crate::ports::UnreadableWebhook;
    use crate::test::empty_media_folder;
    use crate::test::mock_db_context;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    async fn dependencies(
        read_radarr_webhook: ReadRadarrWebhook,
        movie_calls: Arc<AtomicUsize>,
    ) -> Dependencies {
        Dependencies {
            db_context: mock_db_context().await,
            get_radarr_movie: GetRadarrMovie::new(move |_| {
                movie_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(RadarrError::caused_by("Radarr is down")) })
            }),
            media_folder: empty_media_folder(),
            read_radarr_webhook,
            source_health: SourceHealth::default(),
        }
    }

    #[tokio::test]
    async fn an_unreadable_body_is_skipped() {
        let movie_calls = Arc::new(AtomicUsize::new(0));
        let dependencies = dependencies(
            ReadRadarrWebhook::new(|_| Err(UnreadableWebhook::caused_by("missing field `movie`"))),
            movie_calls.clone(),
        )
        .await;

        let skipped = call(
            dependencies,
            Uuid::nil(),
            br#"{"eventType":"Download"}"#.to_vec(),
        )
        .await;

        assert!(skipped.is_ok());
        assert_eq!(movie_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn a_failed_sync_returns_an_error() {
        let movie_calls = Arc::new(AtomicUsize::new(0));
        let dependencies = dependencies(
            ReadRadarrWebhook::new(|_| Ok(RadarrWebhook::Movie { radarr_id: 42 })),
            movie_calls.clone(),
        )
        .await;

        let failed = call(
            dependencies,
            Uuid::nil(),
            br#"{"eventType":"Download"}"#.to_vec(),
        )
        .await;

        assert!(failed.is_err());
        assert_eq!(movie_calls.load(Ordering::Relaxed), 1);
    }
}
