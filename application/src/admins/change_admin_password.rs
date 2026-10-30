mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Admin;
use domain::AdminSession;
use domain::Password;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .admins()
        .filter(Admin::fields().id().eq(id))
        .update()
        .password_hash(password_hash)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .admin_sessions()
        .filter(AdminSession::fields().admin_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
