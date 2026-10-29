use application::ports::MalformedTelemetry;
use application::ports::TelemetrySignal;
use infrastructure::AppState;
use ohno::ErrorExt;
use std::error::Error;
use topcoat::context::Cx;
use topcoat::context::app_context;
use topcoat::router::StatusCode;
use topcoat::router::header::CONTENT_TYPE;
use topcoat::router::request::Bytes;
use topcoat::router::response::IntoResponse;
use topcoat::router::response::Response;
use topcoat::router::route;

#[route(POST "/api/(user)/telemetry/traces/v1")]
pub(crate) async fn traces(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Traces, request).await
}

#[route(POST "/api/(user)/telemetry/metrics/v1")]
pub(crate) async fn metrics(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Metrics, request).await
}

#[route(POST "/api/(user)/telemetry/logs/v1")]
pub(crate) async fn logs(cx: &Cx, request: Bytes) -> topcoat::Result<Response> {
    relay(cx, TelemetrySignal::Logs, request).await
}

async fn relay(cx: &Cx, signal: TelemetrySignal, request: Bytes) -> topcoat::Result<Response> {
    match application::relay_telemetry::call(app_context::<AppState>(cx), signal, request).await {
        Ok(response) => (
            StatusCode::OK,
            [(CONTENT_TYPE, "application/x-protobuf")],
            response,
        )
            .into_response(cx),
        Err(error) if error.find_source::<MalformedTelemetry>().is_some() => {
            StatusCode::BAD_REQUEST.into_response(cx)
        }
        Err(error) => {
            tracing::warn!(error = &error as &dyn Error, "failed to relay telemetry");
            StatusCode::BAD_GATEWAY.into_response(cx)
        }
    }
}
