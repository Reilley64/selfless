mod outbox;
mod session_cleanup;

use application::sessions::delete_expired_sessions;
use infrastructure::AppState;
use tokio_util::sync::CancellationToken;

pub async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    session_cleanup::start(state).await
}

pub async fn serve(state: AppState, cancellation: CancellationToken) {
    tokio::join!(
        session_cleanup::serve(state.clone(), cancellation.clone()),
        outbox::serve(state, cancellation),
    );
}
