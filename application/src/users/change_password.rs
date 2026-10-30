mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::WrongCurrentPassword;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Password;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    token: &str,
    current_password: String,
    new_password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = db_context
        .users()
        .find(user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if !passwords::verify(current_password, user.map(|x| x.password_hash))
        .await
        .map_err(Error::caused_by)?
    {
        return Err(Error::caused_by(WrongCurrentPassword::new()));
    }
    let password_hash = passwords::hash(new_password)
        .await
        .map_err(Error::caused_by)?;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .users()
        .filter(User::fields().id().eq(user_id))
        .update()
        .password_hash(password_hash)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .sessions()
        .filter(
            Session::fields()
                .user_id()
                .eq(user_id)
                .and(Session::fields().token_hash().ne(tokens::hash(token))),
        )
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
