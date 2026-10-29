use application::BuildInfo;
use application::DbContext;
use application::InboxWakeup;
use application::WebhookPasswords;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::GetSonarrShow;
use jiff::Timestamp;

pub use persistence;
pub use sonarr;
pub use telemetry;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub build_info: BuildInfo,
    pub clock: Clock,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
    pub get_sonarr_show: GetSonarrShow,
    pub inbox_wakeup: InboxWakeup,
    pub webhook_passwords: WebhookPasswords,
}

pub fn system_clock() -> Clock {
    Clock::new(Timestamp::now)
}
