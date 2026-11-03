use crate::ports::AcquireLock;
use crate::ports::PortFuture;
use crate::ports::SendEmail;
use crate::ports::SendEmailError;
use crate::ports::persistence::DbScope;
use crate::trace_context;
use domain::EmailAddress;
use domain::Invite;
use domain::OutboxData;
use uuid::Uuid;

pub(crate) fn lock_key(invite_id: Uuid) -> String {
    AcquireLock::key("invites", "email", 1, [invite_id.to_string()])
}

pub(crate) async fn queue(
    db_scope: &mut impl DbScope,
    invite_id: Uuid,
    origin: String,
) -> toasty::Result<()> {
    db_scope
        .outboxes()
        .create()
        .data(OutboxData::SendInviteEmail { invite_id, origin })
        .traceparent(trace_context::current_traceparent())
        .exec(db_scope.executor_mut())
        .await?;
    Ok(())
}

pub(crate) fn send(
    send_email: &SendEmail,
    recipient: EmailAddress,
    origin: &str,
    token: &str,
) -> PortFuture<Result<(), SendEmailError>> {
    let days = Invite::VALID_DAYS;
    send_email.call(
        recipient,
        "You're invited to selfless".to_owned(),
        format!(
            "You're invited to selfless.\n\nOpen this link to choose your name and password:\n{origin}/invite#{token}\n\nThe link works once and expires in {days} days.\n"
        ),
    )
}
