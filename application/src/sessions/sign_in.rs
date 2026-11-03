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
        colour: user.colour(),
        name: user.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use crate::users::create_user;
    use domain::EmailAddress;
    use domain::Password;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use opentelemetry::logs::AnyValue;
    use opentelemetry::logs::Severity;
    use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
    use opentelemetry_sdk::logs::InMemoryLogExporter;
    use opentelemetry_sdk::logs::SdkLoggerProvider;
    use test_fixtures::FakeClock;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const PASSWORD: &str = "a long passphrase";

    struct Household {
        db_context: DbContext,
        clock: FakeClock,
    }

    impl Household {
        async fn new() -> Self {
            Self {
                db_context: mock_db_context().await,
                clock: FakeClock::new(),
            }
        }

        async fn create_user(&self, name: &str) {
            create_user::call(
                create_user::Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(FakeClock::new().reader()),
                },
                name.to_owned(),
                Password::new(PASSWORD.to_owned()).unwrap(),
                EmailAddress::new(format!("{name}@example.com")).unwrap(),
            )
            .await
            .unwrap();
        }

        async fn sign_in(
            &self,
            name: &str,
            password: &str,
            device_name: &str,
            client_id: Uuid,
        ) -> Output {
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                },
                name.to_owned(),
                password.to_owned(),
                ClientKind::AppleTv,
                device_name.to_owned(),
                client_id,
            )
            .await
            .unwrap()
        }

        async fn token(&self, name: &str, device_name: &str, client_id: Uuid) -> String {
            match self.sign_in(name, PASSWORD, device_name, client_id).await {
                Output::SignedIn { token, .. } => token,
                Output::WrongNameOrPassword => panic!("wrong name or password"),
            }
        }

        async fn sessions(&self) -> Vec<Session> {
            let mut db_context = self.db_context.clone();
            db_context
                .sessions()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
        }

        async fn token_hashes(&self) -> Vec<Vec<u8>> {
            sorted(self.sessions().await.into_iter().map(|x| x.token_hash))
        }
    }

    fn sorted(hashes: impl IntoIterator<Item = Vec<u8>>) -> Vec<Vec<u8>> {
        let mut hashes = hashes.into_iter().collect::<Vec<_>>();
        hashes.sort();
        hashes
    }

    #[tokio::test]
    async fn a_wrong_password_changes_no_session() {
        let household = Household::new().await;
        household.create_user("Noor").await;
        let client_id = Uuid::new_v4();
        household.token("Noor", "Living Room", client_id).await;
        let before = household.sessions().await;
        household.clock.advance(SignedDuration::from_hours(2));

        let output = household
            .sign_in("Noor", "wrong", "Bedroom", client_id)
            .await;

        assert!(matches!(output, Output::WrongNameOrPassword));
        let summary = |sessions: Vec<Session>| {
            sessions
                .into_iter()
                .map(|x| (x.id, x.device_name, x.token_hash, x.signed_in_at))
                .collect::<Vec<(Uuid, String, Vec<u8>, Timestamp)>>()
        };
        assert_eq!(summary(household.sessions().await), summary(before));
    }

    #[tokio::test]
    async fn an_unknown_name_returns_wrong_name_or_password() {
        let household = Household::new().await;
        household.create_user("Noor").await;

        let output = household
            .sign_in("Maya", PASSWORD, "Bedroom", Uuid::new_v4())
            .await;

        assert!(matches!(output, Output::WrongNameOrPassword));
        assert!(household.sessions().await.is_empty());
    }

    #[tokio::test]
    async fn the_database_stores_only_the_hash_of_the_token() {
        let household = Household::new().await;
        household.create_user("Noor").await;

        let token = household.token("Noor", "Living Room", Uuid::new_v4()).await;

        assert_eq!(household.token_hashes().await, [tokens::hash(&token)]);
    }

    #[tokio::test]
    async fn a_failed_sign_in_logs_a_warning_without_the_name() {
        let household = Household::new().await;
        let exporter = InMemoryLogExporter::default();
        let provider = SdkLoggerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(OpenTelemetryTracingBridge::new(&provider)),
        );

        household
            .sign_in("Maya", "wrong", "Bedroom", Uuid::new_v4())
            .await;

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
            Some(&AnyValue::from("user sign-in failed"))
        );
        let attributes = format!("{:?}", warning.record.attributes_iter().collect::<Vec<_>>());
        assert!(!attributes.contains("Maya"), "{attributes}");
    }

    #[tokio::test]
    async fn signing_in_again_as_the_same_user_with_the_same_client_id_replaces_the_session() {
        let household = Household::new().await;
        household.create_user("Noor").await;
        let client_id = Uuid::new_v4();
        household.token("Noor", "Living Room", client_id).await;
        let [first] = household.sessions().await.try_into().unwrap();
        household.clock.advance(SignedDuration::from_hours(2));

        let new = household.token("Noor", "Bedroom", client_id).await;

        let [session] = household.sessions().await.try_into().unwrap();
        assert_eq!(session.id, first.id);
        assert_eq!(session.user_id, first.user_id);
        assert_eq!(session.client_id, client_id);
        assert_eq!(session.device_name, "Bedroom");
        assert_eq!(session.token_hash, tokens::hash(&new));
        assert_eq!(session.signed_in_at, household.clock.now());
        assert_eq!(session.last_used_at, household.clock.now());
    }

    #[tokio::test]
    async fn signing_in_with_a_different_client_id_keeps_both_sessions() {
        let household = Household::new().await;
        household.create_user("Noor").await;
        let living_room = household.token("Noor", "Living Room", Uuid::new_v4()).await;

        let bedroom = household.token("Noor", "Bedroom", Uuid::new_v4()).await;

        assert_eq!(
            household.token_hashes().await,
            sorted([tokens::hash(&living_room), tokens::hash(&bedroom)])
        );
    }

    #[tokio::test]
    async fn another_user_signing_in_with_the_same_client_id_keeps_both_sessions() {
        let household = Household::new().await;
        household.create_user("Noor").await;
        household.create_user("Theo").await;
        let client_id = Uuid::new_v4();
        let noor = household.token("Noor", "Living Room", client_id).await;

        let theo = household.token("Theo", "Living Room", client_id).await;

        assert_eq!(
            household.token_hashes().await,
            sorted([tokens::hash(&noor), tokens::hash(&theo)])
        );
    }
}
