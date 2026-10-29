use crate::DbContext;
use domain::Admin;
use domain::AdminSession;
use std::fmt;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to delete an admin")]
pub struct Error;

#[derive(Debug)]
pub struct LastAdmin;

impl fmt::Display for LastAdmin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the last admin cannot be deleted")
    }
}

impl std::error::Error for LastAdmin {}

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    if Admin::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .iter()
        .all(|x| x.id == id)
    {
        return Err(Error::caused_by(LastAdmin));
    }
    Admin::filter_by_id(id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    AdminSession::filter_by_admin_id(id)
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
