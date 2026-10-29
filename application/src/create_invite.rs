use crate::DbContext;
use crate::DbScope;
use crate::invite_email;
use crate::ports::Clock;
use crate::ports::SendEmail;
use crate::tokens;
use crate::unique_index;
use domain::Invite;
use domain::User;
use uuid::Uuid;

const EMAIL_INDEX: &str = "index_invites_by_email";

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
    pub send_email: SendEmail,
}

#[ohno::error]
#[display("failed to create an invite")]
pub struct Error;

#[ohno::error]
#[display("a user or a pending invite already has this email address")]
pub struct AddressTaken;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    admin_id: Uuid,
    email: String,
    origin: String,
) -> Result<(), Error> {
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
    transaction
        .invites()
        .create()
        .email(email.clone())
        .token_hash(tokens::hash(&token))
        .admin_id(admin_id)
        .sent_at(sent_at)
        .expires_at(sent_at + Invite::VALID_FOR)
        .exec(transaction.executor_mut())
        .await
        .map_err(|error| match unique_index::violated(&error) {
            Some(EMAIL_INDEX) => Error::caused_by(AddressTaken::new()),
            _ => Error::caused_by(error),
        })?;
    invite_email::send(&send_email, email, &origin, &token)
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)
}
