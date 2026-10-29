use crate::DbContext;
use crate::DbScope;
use crate::create_user;
use crate::ports::Clock;
use crate::tokens;
use domain::Invite;
use domain::User;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to accept an invite")]
pub struct Error;

#[ohno::error]
#[display("no pending invite has this token, or it has expired")]
pub struct InviteNotFound;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
    name: String,
    password: String,
) -> Result<User, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let Some(invite) = transaction
        .invites()
        .filter(Invite::fields().token_hash().eq(tokens::hash(token)))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| now < x.expires_at)
    else {
        return Err(Error::caused_by(InviteNotFound::new()));
    };
    let user =
        match create_user::create(&mut transaction, name, password, invite.email.clone()).await {
            Ok(user) => user,
            Err(error) => {
                drop(transaction);
                let accepted = db_context
                    .users()
                    .filter(User::fields().email().eq(invite.email))
                    .first()
                    .exec(db_context.executor_mut())
                    .await
                    .map_err(Error::caused_by)?
                    .is_some();
                return Err(if accepted {
                    Error::caused_by(InviteNotFound::new())
                } else {
                    Error::caused_by(error)
                });
            }
        };
    invite
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(user)
}
