use crate::DbContext;
use crate::DbScope;
use crate::passwords;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to create the first admin")]
pub struct Error;

#[ohno::error]
#[display("no admin exists, and the first admin's name or password is missing")]
pub struct MissingFirstAdmin;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: Option<String>,
    password: Option<String>,
) -> Result<(), Error> {
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
        return Ok(());
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
    Ok(())
}
