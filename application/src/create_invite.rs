use crate::DbContext;
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
) -> Result<Uuid, Error> {
    let Dependencies {
        mut db_context,
        clock,
        send_email,
    } = dependencies.into();
    let user = User::filter_by_email(email.clone())
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if user.is_some() {
        return Err(Error::caused_by(AddressTaken::new()));
    }
    let token = tokens::generate();
    let sent_at = clock.call();
    let invite = toasty::create!(Invite {
        email: email.clone(),
        token_hash: tokens::hash(&token),
        admin_id,
        sent_at,
        expires_at: sent_at + Invite::VALID_FOR,
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(|error| match unique_index::violated(&error) {
        Some(EMAIL_INDEX) => Error::caused_by(AddressTaken::new()),
        _ => Error::caused_by(error),
    })?;
    invite_email::send(&send_email, email, &origin, &token)
        .await
        .map_err(Error::caused_by)?;
    Ok(invite.id)
}
