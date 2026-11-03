mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut admins = db_context
        .admins()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    admins.sort_by_cached_key(|x| x.name.to_lowercase());
    Ok(Output {
        admins: admins
            .into_iter()
            .map(|x| output::Admin {
                id: x.id,
                name: x.name,
                created_at: x.created_at,
                last_signed_in_at: x.last_signed_in_at,
            })
            .collect(),
    })
}
