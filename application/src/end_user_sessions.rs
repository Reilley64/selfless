use crate::DbContext;
use domain::Session;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to end a user's sessions")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, user_id: Uuid) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    Session::filter_by_user_id(user_id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
