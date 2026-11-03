mod radarr;
mod sonarr;

use crate::QUEUE_PAUSES;
use domain::Source;
use infrastructure::AppState;
use tokio_util::sync::CancellationToken;

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
        QUEUE_PAUSES,
        cancellation,
    )
    .await;
}
