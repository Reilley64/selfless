use crate::DbContext;
use crate::passwords;
use domain::Admin;
use std::fmt;

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
    if Admin::filter_by_name(name.clone())
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .is_some()
    {
        return Err(Error::caused_by(NameTaken));
    }
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    toasty::create!(Admin {
        name,
        password_hash
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)
}
