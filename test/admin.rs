use application::admins::create_admin;
use application::invites::send_invite_email;
use application::ports::persistence::DbScope;
use application::sessions::get_current_user;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ClientKind;
use domain::EmailAddress;
use domain::OutboxData;
use domain::Password;
use opentelemetry::logs::AnyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use test_app::TestApp;
use test_fixtures::Browser;
use test_fixtures::TestServer;
use topcoat::asset::AssetBundle;
use topcoat::router::Router;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

pub(crate) const ADMIN_NAME: &str = "Maya";
pub(crate) const ADMIN_PASSWORD: &str = "correct horse battery staple";
pub(crate) const USER_NAME: &str = "Noor";
pub(crate) const USER_PASSWORD: &str = "a long passphrase";

pub(crate) fn assets() -> AssetBundle {
    AssetBundle::load_dir(
        std::env::current_exe()
            .unwrap()
            .parent()
            .and_then(|x| x.parent())
            .unwrap()
            .join("assets"),
    )
    .expect("`mise run bundle` writes the asset bundle")
}

pub(crate) async fn serve(app: &TestApp) -> TestServer {
    TestServer::serve(
        crate::register(Router::builder(), assets())
            .app_context(app.state.clone())
            .build(),
    )
    .await
}

pub(crate) async fn create_admin(app: &TestApp, name: &str, password: &str) -> Uuid {
    create_admin::call(
        &app.state,
        name.to_owned(),
        Password::new(password.to_owned()).unwrap(),
    )
    .await
    .unwrap()
    .id
}

pub(crate) async fn signed_in(server: &TestServer, name: &str, password: &str) -> Browser {
    let mut browser = server.browser();
    let page = browser.sign_in(name, password).await;
    assert_eq!(page.status, http::StatusCode::SEE_OTHER, "{name} signs in");
    browser
}

pub(crate) async fn create_user(app: &TestApp, name: &str) -> Uuid {
    create_user::call(
        &app.state,
        name.to_owned(),
        Password::new(USER_PASSWORD.to_owned()).unwrap(),
        EmailAddress::new(format!(
            "{}@example.com",
            name.to_lowercase().replace(' ', ".")
        ))
        .unwrap(),
    )
    .await
    .unwrap()
    .id
}

pub(crate) async fn user_signs_in(
    app: &TestApp,
    name: &str,
    password: &str,
    client_kind: ClientKind,
    device_name: &str,
) -> Option<String> {
    match sign_in::call(
        &app.state,
        name.to_owned(),
        password.to_owned(),
        client_kind,
        device_name.to_owned(),
        Uuid::new_v4(),
    )
    .await
    .unwrap()
    {
        sign_in::Output::SignedIn { token, .. } => Some(token),
        sign_in::Output::WrongNameOrPassword => None,
    }
}

pub(crate) async fn current_user_name(app: &TestApp, token: &str) -> Option<String> {
    match get_current_user::call(&app.state, token).await.unwrap() {
        get_current_user::Output::SignedIn { name, .. } => Some(name),
        get_current_user::Output::NotSignedIn => None,
    }
}

pub(crate) async fn send_invite_emails(app: &TestApp) {
    let mut db_context = app.state.db_context.clone();
    let outboxes = db_context
        .outboxes()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    for outbox in outboxes {
        let OutboxData::SendInviteEmail { invite_id, origin } = outbox.data.clone();
        send_invite_email::call(&app.state, invite_id, origin)
            .await
            .unwrap();
        outbox
            .delete()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }
}

pub(crate) struct Logs {
    exporter: InMemoryLogExporter,
    _tracing: DefaultGuard,
}

impl Logs {
    pub(crate) fn capture() -> Self {
        let exporter = InMemoryLogExporter::default();
        let provider = SdkLoggerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(OpenTelemetryTracingBridge::new(&provider)),
        );
        Self { exporter, _tracing }
    }

    pub(crate) fn admin_actions(&self) -> Vec<(String, String)> {
        self.exporter
            .get_emitted_logs()
            .unwrap()
            .iter()
            .filter(|x| x.record.body() == Some(&AnyValue::from("admin action")))
            .map(|x| {
                let attribute = |key: &str| {
                    x.record
                        .attributes_iter()
                        .find(|(name, _)| name.as_str() == key)
                        .map(|(_, value)| match value {
                            AnyValue::String(value) => value.to_string(),
                            value => format!("{value:?}"),
                        })
                        .unwrap()
                };
                (
                    attribute("selfless.admin.action"),
                    attribute("selfless.admin.target.id"),
                )
            })
            .collect()
    }

    pub(crate) fn assert_none_contains(&self, secrets: &[&str]) {
        for log in self.exporter.get_emitted_logs().unwrap() {
            let record = format!(
                "{:?} {:?}",
                log.record.body(),
                log.record.attributes_iter().collect::<Vec<_>>()
            );
            for secret in secrets {
                assert!(!record.contains(secret), "{record}");
            }
        }
    }
}
