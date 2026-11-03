mod image_cache_cleanup;
mod inbox;
mod inbox_cleanup;
mod outbox;
mod reconcile;
mod session_cleanup;

use application::sessions::delete_expired_sessions;
use infrastructure::AppState;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    session_cleanup::start(state).await
}

pub async fn serve(state: AppState, reconcile_interval: Duration, cancellation: CancellationToken) {
    tokio::join!(
        session_cleanup::serve(state.clone(), cancellation.clone()),
        outbox::serve(state.clone(), cancellation.clone()),
        inbox::serve(state.clone(), cancellation.clone()),
        inbox_cleanup::serve(state.clone(), cancellation.clone()),
        image_cache_cleanup::serve(state.clone(), cancellation.clone()),
        reconcile::serve(state, reconcile_interval, cancellation),
    );
}
