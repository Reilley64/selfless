mod support;

use opentelemetry::Value;
use opentelemetry::logs::AnyValue;
use opentelemetry::logs::Severity;
use opentelemetry::trace::TracerProvider;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME;
use support::USER_PASSWORD;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

#[tokio::test]
async fn admin_request_spans_carry_the_admin_id() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
    );
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    browser.get("/admin/sync").await;

    let spans = exporter.get_finished_spans().unwrap();
    let sync = spans
        .iter()
        .find(|x| x.name == "GET /admin/sync")
        .expect("a span for the sync status page");
    let admin_id = sync
        .attributes
        .iter()
        .find(|x| x.key.as_str() == "selfless.admin.id")
        .expect("the selfless.admin.id attribute");
    let Value::String(admin_id) = &admin_id.value else {
        panic!("selfless.admin.id is a string");
    };
    assert_eq!(
        Uuid::parse_str(admin_id.as_str())
            .unwrap()
            .get_version_num(),
        7
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_failed_admin_sign_in_logs_a_warning_without_the_name() {
    let exporter = InMemoryLogExporter::default();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(OpenTelemetryTracingBridge::new(&provider)),
    );
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    browser.sign_in("Noor", "wrong").await;

    let logs = exporter.get_emitted_logs().unwrap();
    let warnings = logs
        .iter()
        .filter(|x| {
            x.record.severity_number() == Some(Severity::Warn)
                && x.record.body() == Some(&AnyValue::from("admin sign-in failed"))
        })
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    let record = &warnings[0].record;
    let attributes = format!("{:?}", record.attributes_iter().collect::<Vec<_>>());
    assert!(!attributes.contains("Noor"), "{attributes}");
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn each_users_page_action_logs_its_action_and_target_without_the_email_address() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let user_id = server.create_user(USER_NAME, USER_PASSWORD).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    let exporter = InMemoryLogExporter::default();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(OpenTelemetryTracingBridge::new(&provider)),
    );
    let email = "ravi@household.example";

    let users = browser.get("/admin/users").await;
    let csrf = users.csrf_token();
    let invited = browser
        .post_form(
            &users.form_action("Send invite"),
            &[("csrf", &csrf), ("email", email)],
        )
        .await;
    let resend = invited.form_action("Resend");
    let invite_id = resend.split('/').nth(4).unwrap().to_owned();
    browser.post_form(&resend, &[("csrf", &csrf)]).await;
    browser
        .post_form(&invited.form_action("Cancel invite"), &[("csrf", &csrf)])
        .await;
    let user = browser.get(&users.link(USER_NAME)).await;
    browser
        .post_form(
            &user.form_action("Reset"),
            &[("csrf", &csrf), ("password", "a new passphrase")],
        )
        .await;
    browser
        .post_form(
            &user.form_action("Delete"),
            &[("csrf", &csrf), ("name", USER_NAME)],
        )
        .await;

    let logs = exporter.get_emitted_logs().unwrap();
    let actions = logs
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
        .collect::<Vec<_>>();
    let user_id = user_id.to_string();
    assert_eq!(
        actions,
        [
            ("invite.create".to_owned(), invite_id.clone()),
            ("invite.resend".to_owned(), invite_id.clone()),
            ("invite.cancel".to_owned(), invite_id),
            ("user.reset_password".to_owned(), user_id.clone()),
            ("user.delete".to_owned(), user_id),
        ]
    );
    for log in &logs {
        let record = format!(
            "{:?} {:?}",
            log.record.body(),
            log.record.attributes_iter().collect::<Vec<_>>()
        );
        assert!(!record.contains(email), "{record}");
    }
    drop(server);
    database.drop().await;
}
