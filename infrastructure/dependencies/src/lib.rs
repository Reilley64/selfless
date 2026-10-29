use application::BuildInfo;
use application::DbContext;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use jiff::Timestamp;

pub use persistence;
pub use telemetry;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub build_info: BuildInfo,
    pub clock: Clock,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
}

pub fn system_clock() -> Clock {
    Clock::new(Timestamp::now)
}
