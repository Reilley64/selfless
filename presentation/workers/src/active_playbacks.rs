use application::playback::record_active_playbacks;
use application::trace_context;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const PERIOD: Duration = Duration::from_secs(15);

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut ticks = tokio::time::interval(PERIOD);
    while cancellation
        .run_until_cancelled(ticks.tick())
        .await
        .is_some()
    {
        let span = tracing::info_span!(parent: None, "record_active_playbacks");
        let _ = span.set_parent(trace_context::unsampled_parent());
        if let Err(error) = record_active_playbacks::call(&state).instrument(span).await {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to record the active playbacks"
            );
        }
    }
}
