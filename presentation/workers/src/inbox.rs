mod radarr;
mod sonarr;

use domain::Source;
use infrastructure::AppState;
use persistence::QueuePauses;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(100),
    after_failure: Duration::from_secs(30),
};

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    persistence::consume_inbox(
        state.db_context.clone(),
        move |id, source, body| {
            let state = state.clone();
            async move {
                match source {
                    Source::Radarr => radarr::process_delivery(&state, id, body).await,
                    Source::Sonarr => sonarr::process_delivery(&state, id, body).await,
                }
            }
        },
        PAUSES,
        cancellation,
    )
    .await;
}
