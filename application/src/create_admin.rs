use crate::DbContext;
use crate::passwords;
use crate::unique_index;
use domain::Admin;
use std::fmt;

const NAME_INDEX: &str = "index_admins_by_name";

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to create an admin")]
pub struct Error;

#[derive(Debug)]
pub struct NameTaken;

impl fmt::Display for NameTaken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an admin with this name already exists")
    }
}

impl std::error::Error for NameTaken {}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
) -> Result<Admin, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    toasty::create!(Admin {
        name,
        password_hash
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(|error| match unique_index::violated(&error) {
        Some(NAME_INDEX) => Error::caused_by(NameTaken),
        _ => Error::caused_by(error),
    })
}
