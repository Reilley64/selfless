use crate::DbContext;
use crate::create_user::NAME_INDEX;
use crate::create_user::NameTaken;
use crate::unique_index;
use domain::User;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to rename a user")]
pub struct Error;

#[ohno::error]
#[display("the name is empty")]
pub struct EmptyName;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    name: String,
) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    if name.is_empty() {
        return Err(Error::caused_by(EmptyName::new()));
    }
    User::filter_by_id(user_id)
        .update()
        .name(name)
        .exec(db_context.executor_mut())
        .await
        .map_err(|error| match unique_index::violated(&error) {
            Some(NAME_INDEX) => Error::caused_by(NameTaken::new()),
            _ => Error::caused_by(error),
        })
}
