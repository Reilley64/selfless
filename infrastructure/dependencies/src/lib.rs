use application::BuildInfo;
use application::DbContext;
use application::ports::ExportTelemetry;

pub use persistence;
pub use telemetry;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub build_info: BuildInfo,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
}
