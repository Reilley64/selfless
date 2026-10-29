use crate::DbContext;
use crate::DbScope;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to check the database")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    toasty::sql::query("SELECT 1")
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(())
}
