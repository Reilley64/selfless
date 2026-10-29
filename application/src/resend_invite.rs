use crate::DbContext;
use crate::invite_email;
use crate::ports::Clock;
use crate::ports::SendEmail;
use crate::tokens;
use domain::Invite;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub send_email: SendEmail,
}

#[ohno::error]
#[display("failed to resend an invite")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    origin: String,
) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        clock,
        send_email,
    } = dependencies.into();
    let Some(mut invite) = Invite::filter_by_id(id)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(());
    };
    let token = tokens::generate();
    let sent_at = clock.call();
    invite
        .update()
        .token_hash(tokens::hash(&token))
        .sent_at(sent_at)
        .expires_at(sent_at + Invite::VALID_FOR)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    invite_email::send(&send_email, invite.email, &origin, &token)
        .await
        .map_err(Error::caused_by)
}
