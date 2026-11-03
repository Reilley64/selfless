use application::ports::ReadSonarrWebhook;
use application::ports::SonarrWebhook;
use application::ports::UnreadableWebhook;
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
                | SonarrEvent::EpisodeFileDelete { series } => SonarrWebhook::Show {
                    sonarr_id: series.id,
                },
                SonarrEvent::Test
                | SonarrEvent::Grab
                | SonarrEvent::Health
                | SonarrEvent::HealthRestored
                | SonarrEvent::ApplicationUpdate
                | SonarrEvent::ManualInteractionRequired
                | SonarrEvent::Other => SonarrWebhook::Ignored,
            },
        )
    })
}
