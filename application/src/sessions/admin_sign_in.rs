mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::selfless_auth_client_kind;
use crate::ports::persistence::DbScope;
use crate::sessions::sign_in_attempts;
use crate::tokens;
use domain::Admin;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let admin = db_context
        .admins()
        .filter(Admin::fields().name().eq(name))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let matches = sign_in_attempts::check_and_count(
        password,
        admin.as_ref().map(|x| x.password_hash.clone()),
        selfless_auth_client_kind::ADMIN,
    )
    .await
    .map_err(Error::caused_by)?;
    let Some(admin) = admin.filter(|_| matches) else {
        tracing::warn!("admin sign-in failed");
        return Ok(Output::WrongNameOrPassword);
    };
    let token = tokens::generate();
    let now = clock.call();
    db_context
        .admin_sessions()
        .create()
        .admin_id(admin.id)
        .token_hash(tokens::hash(&token))
        .signed_in_at(now)
        .last_used_at(now)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output::SignedIn { token })
}
