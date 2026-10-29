use crate::DbContext;
use crate::DbScope;
use crate::passwords;
use crate::unique_index;
use domain::Admin;

const NAME_INDEX: &str = "index_admins_by_name";

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to create an admin")]
pub struct Error;

#[ohno::error]
#[display("an admin with this name already exists")]
pub struct NameTaken;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
) -> Result<Admin, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    db_context
        .admins()
        .create()
        .name(name)
        .password_hash(password_hash)
        .exec(db_context.executor_mut())
        .await
        .map_err(|error| match unique_index::violated(&error) {
            Some(NAME_INDEX) => Error::caused_by(NameTaken::new()),
            _ => Error::caused_by(error),
        })
}
