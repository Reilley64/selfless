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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admins::create_admin;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::Password;
    use opentelemetry::logs::AnyValue;
    use opentelemetry::logs::Severity;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_sdk::logs::InMemoryLogExporter;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use test_fixtures::FakeClock;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const PASSWORD: &str = "a long passphrase";

    async fn create_admin(db_context: &DbContext, name: &str) {
        create_admin::call(
            create_admin::Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap();
    }

    async fn admin_sign_in(db_context: &DbContext, name: &str, password: &str) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            password.to_owned(),
        )
        .await
        .unwrap()
    }

    async fn token_hashes(db_context: &mut DbContext) -> Vec<Vec<u8>> {
        db_context
            .admin_sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect()
    }

    #[tokio::test]
    async fn a_wrong_name_and_a_wrong_password_both_return_wrong_name_or_password() {
        let mut db_context = mock_db_context().await;
        create_admin(&db_context, "Maya").await;

        let wrong_name = admin_sign_in(&db_context, "Noor", PASSWORD).await;
        let wrong_password = admin_sign_in(&db_context, "Maya", "wrong").await;

        assert!(matches!(wrong_name, Output::WrongNameOrPassword));
        assert!(matches!(wrong_password, Output::WrongNameOrPassword));
        assert!(token_hashes(&mut db_context).await.is_empty());
    }

    #[tokio::test]
    async fn the_database_stores_only_the_hash_of_the_admin_session_token() {
        let mut db_context = mock_db_context().await;
        create_admin(&db_context, "Maya").await;

        let Output::SignedIn { token } = admin_sign_in(&db_context, "Maya", PASSWORD).await else {
            panic!("wrong name or password");
        };

        assert_eq!(token_hashes(&mut db_context).await, [tokens::hash(&token)]);
    }

    #[tokio::test]
    async fn a_failed_admin_sign_in_logs_a_warning_without_the_name() {
        let db_context = mock_db_context().await;
        let exporter = InMemoryLogExporter::default();
        let provider = SdkLoggerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(OpenTelemetryTracingBridge::new(&provider)),
        );

        admin_sign_in(&db_context, "Noor", "wrong").await;

        let logs = exporter.get_emitted_logs().unwrap();
        let warnings = logs
            .iter()
            .filter(|x| x.record.severity_number() == Some(Severity::Warn))
            .collect::<Vec<_>>();
        let [warning] = warnings.as_slice() else {
            panic!("one warning, got {warnings:?}");
        };
        assert_eq!(
            warning.record.body(),
            Some(&AnyValue::from("admin sign-in failed"))
        );
        let attributes = format!("{:?}", warning.record.attributes_iter().collect::<Vec<_>>());
        assert!(!attributes.contains("Noor"), "{attributes}");
    }
}
