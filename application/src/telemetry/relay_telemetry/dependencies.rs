use crate::ports::ExportTelemetry;

#[fundle::deps]
pub struct Dependencies {
    pub export_telemetry: ExportTelemetry,
}
