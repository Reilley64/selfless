use crate::DbContext;
use crate::attributes::SELFLESS_AUTH_CLIENT_KIND;
use crate::attributes::SELFLESS_AUTH_OUTCOME;
use crate::metrics::SELFLESS_AUTH_SIGN_INS;
use crate::passwords;
use crate::ports::Clock;
use crate::session_tokens;
use domain::Admin;
use domain::AdminSession;
use opentelemetry::KeyValue;
use opentelemetry::global;

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
    let admin = Admin::filter_by_name(name)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let matches = passwords::verify(password, admin.as_ref().map(|x| x.password_hash.clone()))
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
                KeyValue::new(SELFLESS_AUTH_CLIENT_KIND, "admin"),
            ],
        );
    let Some(admin) = admin.filter(|_| matches) else {
        tracing::warn!("admin sign-in failed");
        return Ok(None);
    };
    let token = session_tokens::generate();
    let now = clock.call();
    toasty::create!(AdminSession {
        admin_id: admin.id,
        token_hash: session_tokens::hash(&token),
        signed_in_at: now,
        last_used_at: now,
    })
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    Ok(Some(token))
}
