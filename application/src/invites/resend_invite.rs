mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::InviteNotFound;
pub use output::Output;

use crate::invites::invite_email;
use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Invite;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    origin: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        send_email,
    } = dependencies.into();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let Some(mut invite) = transaction
        .invites()
        .find(id)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Err(Error::caused_by(InviteNotFound::new()));
    };
    let token = tokens::generate();
    let sent_at = clock.call();
    invite
        .update()
        .token_hash(tokens::hash(&token))
        .sent_at(sent_at)
        .expires_at(sent_at + Invite::VALID_FOR)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    invite_email::send(&send_email, invite.email, &origin, &token)
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
