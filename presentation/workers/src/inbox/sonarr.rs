use infrastructure::AppState;
use persistence::HandleInboxError;
use uuid::Uuid;

pub(super) async fn process_delivery(
    state: &AppState,
    id: Uuid,
    body: Vec<u8>,
) -> Result<(), HandleInboxError> {
    application::webhooks::process_sonarr_delivery::call(state, id, body)
        .await
        .map_err(HandleInboxError::caused_by)?;
    Ok(())
}
