use crate::DbContext;
use crate::attributes::SELFLESS_AUTH_CLIENT_KIND;
use crate::attributes::SELFLESS_AUTH_OUTCOME;
use crate::metrics::SELFLESS_AUTH_SIGN_INS;
use crate::passwords;
use crate::ports::Clock;
use crate::session_tokens;
use domain::ClientKind;
use domain::Session;
use domain::User;
use opentelemetry::KeyValue;
use opentelemetry::global;

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
    let matches = passwords::verify(password, user.as_ref().map(|x| x.password_hash.clone()))
        .await
        .map_err(Error::caused_by)?;
    global::meter("selfless")
        .u64_counter(SELFLESS_AUTH_SIGN_INS)
        .with_unit("{sign_in}")
        .build()
        .add(
            1,
            &[
                KeyValue::new(
                    SELFLESS_AUTH_OUTCOME,
                    if matches { "succeeded" } else { "failed" },
                ),
                KeyValue::new(
                    SELFLESS_AUTH_CLIENT_KIND,
                    match client_kind {
                        ClientKind::Web => "web",
                        ClientKind::AppleTv => "appleTv",
                        ClientKind::GoogleTv => "googleTv",
                    },
                ),
            ],
        );
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
