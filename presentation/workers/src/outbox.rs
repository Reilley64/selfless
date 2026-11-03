mod invites;

use crate::QUEUE_PAUSES;
use domain::OutboxData;
use infrastructure::AppState;
use tokio_util::sync::CancellationToken;

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
        QUEUE_PAUSES,
        cancellation,
    )
    .await;
}
