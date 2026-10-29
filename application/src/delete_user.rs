use crate::DbContext;
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
    User::filter_by_id(id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Session::filter_by_user_id(id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
