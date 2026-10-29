mod session_cleanup;

use infrastructure::AppState;
use tokio_util::sync::CancellationToken;

pub async fn serve(state: AppState, cancellation: CancellationToken) {
    session_cleanup::serve(state, cancellation).await;
}
