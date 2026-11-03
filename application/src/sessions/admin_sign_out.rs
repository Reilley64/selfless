mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::AdminSession;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .admin_sessions()
        .filter(AdminSession::fields().token_hash().eq(tokens::hash(token)))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
