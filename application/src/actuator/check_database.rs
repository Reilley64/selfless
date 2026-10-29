mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    toasty::sql::query("SELECT 1")
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
