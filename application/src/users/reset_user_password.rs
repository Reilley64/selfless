mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    password: String,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
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
        .filter(Session::fields().user_id().eq(user_id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
