use crate::DbContext;
use crate::attributes::selfless_auth_client_kind;
use crate::ports::Clock;
use crate::session_tokens;
use crate::sign_ins;
use domain::ClientKind;
use domain::Session;
use domain::User;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to sign in a user")]
pub struct Error;

pub struct SignedIn {
    pub token: String,
    pub user: User,
}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    client_kind: ClientKind,
    device_name: String,
) -> Result<Option<SignedIn>, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let user = User::filter_by_name(name)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let matches = sign_ins::verify(
        password,
        user.as_ref().map(|x| x.password_hash.clone()),
        match client_kind {
            ClientKind::Web => selfless_auth_client_kind::WEB,
            ClientKind::AppleTv => selfless_auth_client_kind::APPLE_TV,
            ClientKind::GoogleTv => selfless_auth_client_kind::GOOGLE_TV,
        },
    )
    .await
    .map_err(Error::caused_by)?;
    let Some(user) = user.filter(|_| matches) else {
        tracing::warn!("user sign-in failed");
        return Ok(None);
    };
    let token = session_tokens::generate();
    let now = clock.call();
    toasty::create!(Session {
        user_id: user.id,
        client_kind,
        device_name,
        token_hash: session_tokens::hash(&token),
        signed_in_at: now,
        last_used_at: now,
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    Ok(Some(SignedIn { token, user }))
}
