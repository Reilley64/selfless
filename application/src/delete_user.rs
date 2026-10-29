use crate::DbContext;
use crate::DbScope;
use domain::Session;
use domain::User;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to delete a user")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<(), Error> {
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
    transaction.commit().await.map_err(Error::caused_by)
}
