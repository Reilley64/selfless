use crate::DbContext;
use crate::passwords;
use domain::Admin;
use domain::AdminSession;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to change an admin's password")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    password: String,
) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    Admin::filter_by_id(id)
        .update()
        .password_hash(password_hash)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    AdminSession::filter_by_admin_id(id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
