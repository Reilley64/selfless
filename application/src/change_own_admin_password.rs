use crate::DbContext;
use crate::change_password::WrongCurrentPassword;
use crate::passwords;
use crate::tokens;
use domain::Admin;
use domain::AdminSession;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to change an admin's own password")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    admin_id: Uuid,
    token: &str,
    current_password: String,
    new_password: String,
) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    let admin = Admin::filter_by_id(admin_id)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if !passwords::verify(current_password, admin.map(|x| x.password_hash))
        .await
        .map_err(Error::caused_by)?
    {
        return Err(Error::caused_by(WrongCurrentPassword::new()));
    }
    let password_hash = passwords::hash(new_password)
        .await
        .map_err(Error::caused_by)?;
    Admin::filter_by_id(admin_id)
        .update()
        .password_hash(password_hash)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    AdminSession::filter(
        AdminSession::fields()
            .admin_id()
            .eq(admin_id)
            .and(AdminSession::fields().token_hash().ne(tokens::hash(token))),
    )
    .delete()
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)
}
