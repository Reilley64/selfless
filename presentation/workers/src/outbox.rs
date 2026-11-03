mod invites;

use domain::OutboxData;
use infrastructure::AppState;
use persistence::QueuePauses;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(100),
    after_failure: Duration::from_secs(30),
};

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    persistence::consume_outbox(
        state.db_context.clone(),
        move |data| {
            let state = state.clone();
            async move {
                match data {
                    OutboxData::SendInviteEmail { invite_id, origin } => {
                        invites::send_invite_email(&state, invite_id, origin).await
                    }
                }
            }
        },
        PAUSES,
        cancellation,
    )
    .await;
}
