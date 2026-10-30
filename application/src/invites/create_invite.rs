mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::AddressTaken;
pub use error::Error;
pub use output::Output;

use crate::invites::invite_email;
use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::EmailAddress;
use domain::Invite;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    admin_id: Uuid,
    email: EmailAddress,
    origin: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        send_email,
    } = dependencies.into();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let user = transaction
        .users()
        .filter(User::fields().email().eq(email.clone()))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if user.is_some() {
        return Err(Error::caused_by(AddressTaken::new()));
    }
    let token = tokens::generate();
    let sent_at = clock.call();
    Invite::upsert_by_email(email.clone())
        .token_hash(tokens::hash(&token))
        .admin_id(admin_id)
        .sent_at(sent_at)
        .expires_at(sent_at + Invite::VALID_FOR)
        .or_ignore()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(AddressTaken::new()))?;
    invite_email::send(&send_email, email, &origin, &token)
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
