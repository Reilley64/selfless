use crate::ports::ExportTelemetry;
use crate::ports::TelemetrySignal;
use bytes::Bytes;

#[fundle::deps]
pub struct Dependencies {
    pub export_telemetry: ExportTelemetry,
}

#[ohno::error]
#[display("failed to relay telemetry")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    signal: TelemetrySignal,
    request: Bytes,
) -> Result<Bytes, Error> {
    let export_telemetry = dependencies.into().export_telemetry;
    export_telemetry
        .call(signal, request)
        .await
        .map_err(Error::caused_by)
}
