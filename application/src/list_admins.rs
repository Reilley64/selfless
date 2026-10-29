use crate::DbContext;
use domain::Admin;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list the admins")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Admin>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut admins = Admin::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    admins.sort_by_cached_key(|x| x.name.to_lowercase());
    Ok(admins)
}
