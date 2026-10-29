mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::MissingFirstAdmin;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: Option<String>,
    password: Option<String>,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    if db_context
        .admins()
        .all()
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .is_some()
    {
        return Ok(Output);
    }
    let (Some(name), Some(password)) = (
        name.filter(|x| !x.is_empty()),
        password.filter(|x| !x.is_empty()),
    ) else {
        return Err(Error::caused_by(MissingFirstAdmin::new()));
    };
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    db_context
        .admins()
        .create()
        .name(name)
        .password_hash(password_hash)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
