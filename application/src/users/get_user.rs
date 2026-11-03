mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = db_context
        .users()
        .find(id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(match user {
        Some(user) => Output::Found {
            id: user.id,
            name: user.name,
        },
        None => Output::NotFound,
    })
}
