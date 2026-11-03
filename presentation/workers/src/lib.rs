mod active_playbacks;
mod image_cache_cleanup;
mod inbox;
mod inbox_cleanup;
mod open_playbacks;
mod outbox;
mod playback_cleanup;
mod reconcile;
mod session_cleanup;

use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn start(state: &AppState) -> Result<(), Box<dyn Error + Send + Sync>> {
    session_cleanup::start(state).await?;
    open_playbacks::start(state).await?;
    Ok(())
}

pub async fn serve(state: AppState, reconcile_interval: Duration, cancellation: CancellationToken) {
    tokio::join!(
        active_playbacks::serve(state.clone(), cancellation.clone()),
        session_cleanup::serve(state.clone(), cancellation.clone()),
        outbox::serve(state.clone(), cancellation.clone()),
        inbox::serve(state.clone(), cancellation.clone()),
        inbox_cleanup::serve(state.clone(), cancellation.clone()),
        image_cache_cleanup::serve(state.clone(), cancellation.clone()),
        open_playbacks::serve(state.clone(), cancellation.clone()),
        playback_cleanup::serve(state.clone(), cancellation.clone()),
        reconcile::serve(state, reconcile_interval, cancellation),
    );
}
