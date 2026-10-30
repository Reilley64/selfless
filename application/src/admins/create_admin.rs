mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Admin;
use domain::Password;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let admin = Admin::upsert_by_name(name)
        .password_hash(password_hash)
        .or_ignore()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(NameTaken::new()))?;
    Ok(Output { id: admin.id })
}
