mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Invite;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .invites()
        .filter(Invite::fields().id().eq(id))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
