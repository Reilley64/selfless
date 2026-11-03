mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::EmptyName;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    name: String,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    if name.is_empty() {
        return Err(Error::caused_by(EmptyName::new()));
    }
    let taken = db_context
        .users()
        .filter(
            User::fields()
                .name()
                .eq(&name)
                .and(User::fields().id().ne(user_id)),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .is_some();
    if taken {
        return Err(Error::caused_by(NameTaken::new()));
    }
    db_context
        .users()
        .filter(User::fields().id().eq(user_id))
        .update()
        .name(name)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
