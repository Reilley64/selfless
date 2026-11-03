use infrastructure::AppState;
use persistence::HandleOutboxError;
use uuid::Uuid;

pub(super) async fn send_invite_email(
    state: &AppState,
    invite_id: Uuid,
    origin: String,
) -> Result<(), HandleOutboxError> {
    application::invites::send_invite_email::call(state, invite_id, origin)
        .await
        .map_err(HandleOutboxError::caused_by)?;
    Ok(())
}
