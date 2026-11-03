mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::SELFLESS_WEBHOOK_EVENT_TYPE;
use crate::attributes::SELFLESS_WEBHOOK_OUTCOME;
use crate::attributes::SELFLESS_WEBHOOK_SOURCE;
use crate::library::sync_movie;
use crate::ports::MovieWebhookDelivery;
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
        find_media_file,
        get_radarr_movie,
        list_subtitle_files,
        prober,
        read_movie_folder,
        read_movie_nfo,
        read_radarr_webhook,
        read_webhook_event_type,
        source_health,
    } = dependencies.into();
    let span = Span::current();
    let event_type = read_webhook_event_type.call(&body);
    span.set_attribute(SELFLESS_WEBHOOK_SOURCE, Source::Radarr.name());
    if let Some(event_type) = &event_type {
        span.set_attribute(SELFLESS_WEBHOOK_EVENT_TYPE, event_type.clone());
    }
    let synced = match read_radarr_webhook.call(&body) {
        Ok(MovieWebhookDelivery::Movie { radarr_id }) => sync_movie::call(
            sync_movie::Dependencies {
                db_context,
                find_media_file,
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
        .map(|_| Outcome::Processed),
        Ok(MovieWebhookDelivery::Ignored) => Ok(Outcome::Ignored),
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
    use crate::library::SourceHealth;
    use crate::ports::GetRadarrMovie;
    use crate::ports::RadarrError;
    use crate::ports::ReadRadarrWebhook;
    use crate::ports::ReadWebhookEventType;
    use crate::ports::UnreadableWebhook;
    use crate::test::empty_movie_folder;
    use crate::test::failing_prober;
    use crate::test::missing_media_file;
    use crate::test::mock_db_context;
    use crate::test::no_subtitle_files;
    use crate::test::unreadable_movie_nfo;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    async fn dependencies(
        read_radarr_webhook: ReadRadarrWebhook,
        movie_calls: Arc<AtomicUsize>,
    ) -> Dependencies {
        Dependencies {
            db_context: mock_db_context().await,
            find_media_file: missing_media_file(),
            get_radarr_movie: GetRadarrMovie::new(move |_| {
                movie_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(RadarrError::caused_by("Radarr is down")) })
            }),
            list_subtitle_files: no_subtitle_files(),
            prober: failing_prober(),
            read_movie_folder: empty_movie_folder(),
            read_movie_nfo: unreadable_movie_nfo(),
            read_radarr_webhook,
            read_webhook_event_type: ReadWebhookEventType::new(|_| Some("Download".to_owned())),
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
            ReadRadarrWebhook::new(|_| Ok(MovieWebhookDelivery::Movie { radarr_id: 42 })),
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
