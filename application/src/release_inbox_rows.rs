use crate::DbContext;
use crate::unsampled;
use tracing::Instrument;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to release the claimed inbox rows")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    toasty::sql::statement("UPDATE inbox_rows SET status = 'pending' WHERE status = 'processing'")
        .exec(db_context.executor_mut())
        .instrument(unsampled::inbox_poll())
        .await
        .map_err(Error::caused_by)?;
    Ok(())
}
