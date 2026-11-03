mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::TelemetrySignal;
use bytes::Bytes;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    signal: TelemetrySignal,
    request: Bytes,
) -> Result<Output, Error> {
    let export_telemetry = dependencies.into().export_telemetry;
    let response = export_telemetry
        .call(signal, request)
        .await
        .map_err(Error::caused_by)?;
    Ok(Output { response })
}
