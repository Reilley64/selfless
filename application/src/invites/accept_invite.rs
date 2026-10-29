mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::InviteNotFound;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use crate::users::create_user;
use domain::Invite;
use domain::User;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
    name: String,
    password: String,
) -> Result<Output, Error> {
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
        .filter(|x| !x.expired(now))
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
    Ok(Output { user })
}
