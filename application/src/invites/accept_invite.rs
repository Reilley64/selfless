mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::InviteNotFound;
pub use output::Output;

use crate::ports::AcquireLock;
use crate::ports::persistence::DbScope;
use crate::tokens;
use crate::users::create_user;
use domain::Invite;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
    name: String,
    password: String,
    idempotency_key: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        acquire_lock,
    } = dependencies.into();
    let _guard = acquire_lock
        .call(AcquireLock::key(
            "invites",
            "accept",
            1,
            [idempotency_key.to_string()],
        ))
        .await;
    if let Some(user) = db_context
        .users()
        .filter(User::fields().name().eq(&name))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| x.idempotency_key == Some(idempotency_key))
    {
        return Ok(Output {
            id: user.id,
            name: user.name,
            colour: user.colour,
            symbol: user.symbol,
        });
    }
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
    let user = match create_user::create(
        &mut transaction,
        name,
        password,
        invite.email.clone(),
        Some(idempotency_key),
    )
    .await
    {
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
    Ok(Output {
        id: user.id,
        name: user.name,
        colour: user.colour,
        symbol: user.symbol,
    })
}
