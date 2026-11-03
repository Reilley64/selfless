mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::sync_show;
use crate::ports::ShowWebhookDelivery;
use crate::webhooks::webhook_deliveries;
use crate::webhooks::webhook_deliveries::Outcome;
use domain::Source;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    body: Vec<u8>,
) -> Result<Output, Error> {
    let Dependencies {
        db_context,
        find_media_file,
        get_sonarr_show,
        list_subtitle_files,
        prober,
        read_episode_folder,
        read_episode_nfo,
        read_show_folder,
        read_show_nfo,
        read_sonarr_webhook,
        read_webhook_event_type,
        source_health,
    } = dependencies.into();
    webhook_deliveries::process(
        Source::Sonarr,
        id,
        read_webhook_event_type.call(&body),
        read_sonarr_webhook.call(&body),
        async |delivery| match delivery {
            ShowWebhookDelivery::Show { sonarr_id } => sync_show::call(
                sync_show::Dependencies {
                    db_context,
                    find_media_file,
                    get_sonarr_show,
                    list_subtitle_files,
                    prober,
                    read_episode_folder,
                    read_episode_nfo,
                    read_show_folder,
                    read_show_nfo,
                    source_health,
                },
                sonarr_id,
            )
            .await
            .map(|_| Outcome::Processed),
            ShowWebhookDelivery::Ignored => Ok(Outcome::Ignored),
        },
    )
    .await
    .map(|()| Output)
    .map_err(Error::caused_by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::SourceHealth;
    use crate::ports::GetSonarrShow;
    use crate::ports::ReadSonarrWebhook;
    use crate::ports::ReadWebhookEventType;
    use crate::ports::SonarrError;
    use crate::ports::UnreadableWebhook;
    use crate::test::empty_episode_folder;
    use crate::test::empty_show_folder;
    use crate::test::failing_prober;
    use crate::test::missing_media_file;
    use crate::test::mock_db_context;
    use crate::test::no_subtitle_files;
    use crate::test::unreadable_episode_nfo;
    use crate::test::unreadable_show_nfo;
    use domain::SonarrId;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    async fn dependencies(
        read_sonarr_webhook: ReadSonarrWebhook,
        show_calls: Arc<AtomicUsize>,
    ) -> Dependencies {
        Dependencies {
            db_context: mock_db_context().await,
            find_media_file: missing_media_file(),
            get_sonarr_show: GetSonarrShow::new(move |_| {
                show_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Err(SonarrError::caused_by("Sonarr is down")) })
            }),
            list_subtitle_files: no_subtitle_files(),
            prober: failing_prober(),
            read_episode_folder: empty_episode_folder(),
            read_episode_nfo: unreadable_episode_nfo(),
            read_show_folder: empty_show_folder(),
            read_show_nfo: unreadable_show_nfo(),
            read_sonarr_webhook,
            read_webhook_event_type: ReadWebhookEventType::new(|_| Some("Download".to_owned())),
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
            ReadSonarrWebhook::new(|_| {
                Ok(ShowWebhookDelivery::Show {
                    sonarr_id: SonarrId(42),
                })
            }),
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
