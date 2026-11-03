use crate::attributes::SELFLESS_AUTH_CLIENT_KIND;
use crate::attributes::SELFLESS_AUTH_OUTCOME;
use crate::attributes::selfless_auth_outcome;
use crate::metrics::SELFLESS_AUTH_SIGN_INS;
use crate::passwords;
use crate::passwords::PasswordError;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Counter;
use std::sync::LazyLock;

static SIGN_INS: LazyLock<Counter<u64>> = LazyLock::new(|| {
    global::meter("selfless")
        .u64_counter(SELFLESS_AUTH_SIGN_INS)
        .with_unit("{sign_in}")
        .build()
});

pub(crate) async fn check_and_count(
    password: String,
    hash: Option<String>,
    client_kind: &'static str,
) -> Result<bool, PasswordError> {
    let matches = passwords::verify(password, hash).await?;
    SIGN_INS.add(
        1,
        &[
            KeyValue::new(
                SELFLESS_AUTH_OUTCOME,
                if matches {
                    selfless_auth_outcome::SUCCEEDED
                } else {
                    selfless_auth_outcome::FAILED
                },
            ),
            KeyValue::new(SELFLESS_AUTH_CLIENT_KIND, client_kind),
        ],
    );
    Ok(matches)
}
