use application::sessions::delete_expired_sessions;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    delete_expired_sessions::call(state).await?;
    Ok(())
}

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut daily = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(daily.tick())
        .await
        .is_some()
    {
        if let Err(error) = delete_expired_sessions::call(&state).await {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to delete expired sessions"
            );
        }
    }
}
