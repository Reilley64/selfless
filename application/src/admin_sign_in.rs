use crate::DbContext;
use crate::DbScope;
use crate::attributes::selfless_auth_client_kind;
use crate::ports::Clock;
use crate::sign_ins;
use crate::tokens;
use domain::Admin;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to sign in an admin")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
) -> Result<Option<String>, Error> {
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
    let matches = sign_ins::verify(
        password,
        admin.as_ref().map(|x| x.password_hash.clone()),
        selfless_auth_client_kind::ADMIN,
    )
    .await
    .map_err(Error::caused_by)?;
    let Some(admin) = admin.filter(|_| matches) else {
        tracing::warn!("admin sign-in failed");
        return Ok(None);
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
    Ok(Some(token))
}
