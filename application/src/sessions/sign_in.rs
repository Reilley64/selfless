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
use domain::ClientKind;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    client_kind: ClientKind,
    device_name: String,
    client_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let user = db_context
        .users()
        .filter(User::fields().name().eq(name))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let matches = sign_in_attempts::check_and_count(
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
        return Ok(Output::WrongNameOrPassword);
    };
    let token = tokens::generate();
    let now = clock.call();
    Session::upsert_by_user_id_and_client_id(user.id, client_id)
        .client_kind(client_kind)
        .device_name(device_name)
        .token_hash(tokens::hash(&token))
        .signed_in_at(now)
        .last_used_at(now)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output::SignedIn {
        token,
        user_id: user.id,
        name: user.name,
        colour: user.colour,
        symbol: user.symbol,
    })
}
