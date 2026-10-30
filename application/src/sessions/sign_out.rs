mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .sessions()
        .filter(Session::fields().token_hash().eq(tokens::hash(token)))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
