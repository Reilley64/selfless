use application::BuildInfo;
use application::ports::AcquireLock;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::SendEmail;
use application::ports::persistence::DbContext;
use jiff::Timestamp;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub acquire_lock: AcquireLock,
    pub build_info: BuildInfo,
    pub clock: Clock,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
    pub send_email: SendEmail,
}

pub fn system_clock() -> Clock {
    Clock::new(Timestamp::now)
}
