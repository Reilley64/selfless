use application::ports::ReadSonarrWebhook;
use application::ports::ShowWebhookDelivery;
use application::ports::UnreadableWebhook;
use domain::SonarrId;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "eventType")]
enum SonarrEvent {
    Download {
        series: SonarrSeriesRef,
    },
    Rename {
        series: SonarrSeriesRef,
    },
    SeriesAdd {
        series: SonarrSeriesRef,
    },
    SeriesDelete {
        series: SonarrSeriesRef,
    },
    EpisodeFileDelete {
        series: SonarrSeriesRef,
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
struct SonarrSeriesRef {
    id: i64,
}

pub fn read_webhook() -> ReadSonarrWebhook {
    ReadSonarrWebhook::new(|body| {
        Ok(
            match serde_json::from_slice(body).map_err(UnreadableWebhook::caused_by)? {
                SonarrEvent::Download { series }
                | SonarrEvent::Rename { series }
                | SonarrEvent::SeriesAdd { series }
                | SonarrEvent::SeriesDelete { series }
                | SonarrEvent::EpisodeFileDelete { series } => ShowWebhookDelivery::Show {
                    sonarr_id: SonarrId(series.id),
                },
                SonarrEvent::Test
                | SonarrEvent::Grab
                | SonarrEvent::Health
                | SonarrEvent::HealthRestored
                | SonarrEvent::ApplicationUpdate
                | SonarrEvent::ManualInteractionRequired
                | SonarrEvent::Other => ShowWebhookDelivery::Ignored,
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(name: &str) -> Result<ShowWebhookDelivery, UnreadableWebhook> {
        let body = std::fs::read(format!(
            "{}/../../test/fixtures/sonarr/webhooks/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        read_webhook().call(&body)
    }

    #[test]
    fn library_events_name_the_show() {
        for name in [
            "download",
            "download-import-complete",
            "download-upgrade",
            "rename",
            "series-add",
            "series-delete",
            "episode-file-delete",
        ] {
            assert_eq!(
                read(name).unwrap(),
                ShowWebhookDelivery::Show {
                    sonarr_id: SonarrId(7)
                },
                "{name}"
            );
        }
    }

    #[test]
    fn other_events_are_ignored() {
        for name in [
            "test",
            "grab",
            "health",
            "health-restored",
            "application-update",
            "manual-interaction-required",
        ] {
            assert_eq!(read(name).unwrap(), ShowWebhookDelivery::Ignored, "{name}");
        }
    }

    #[test]
    fn a_library_event_without_its_series_is_unreadable() {
        assert!(read_webhook().call(br#"{"eventType":"Download"}"#).is_err());
    }
}
