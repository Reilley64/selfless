mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .users()
        .filter(User::fields().id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .sessions()
        .filter(Session::fields().user_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}
