use application::library::delete_old_inbox_rows;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(60 * 60);

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut hourly = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(hourly.tick())
        .await
        .is_some()
    {
        match delete_old_inbox_rows::call(&state).await {
            Ok(output) if output.deleted > 0 => tracing::info!(
                selfless.webhook.deleted_deliveries = output.deleted,
                "deleted old inbox rows"
            ),
            Ok(_) => {}
            Err(error) => tracing::error!(
                error = &error as &dyn Error,
                "failed to delete old inbox rows"
            ),
        }
    }
}
