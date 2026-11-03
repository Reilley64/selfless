use application::webhooks::process_radarr_delivery;
use application::webhooks::process_sonarr_delivery;
use domain::Source;
use infrastructure::AppState;
use persistence::HandleInboxError;
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
                    Source::Radarr => process_radarr_delivery::call(&state, id, body)
                        .await
                        .map(drop)
                        .map_err(HandleInboxError::caused_by),
                    Source::Sonarr => process_sonarr_delivery::call(&state, id, body)
                        .await
                        .map(drop)
                        .map_err(HandleInboxError::caused_by),
                }
            }
        },
        PAUSES,
        cancellation,
    )
    .await;
}
