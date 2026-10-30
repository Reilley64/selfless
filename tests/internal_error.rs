mod support;

use opentelemetry::logs::AnyValue;
use opentelemetry::logs::Severity;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::json;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::FakeClock;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME;
use support::USER_PASSWORD;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

fn capture_logs(exporter: &InMemoryLogExporter) -> DefaultGuard {
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(OpenTelemetryTracingBridge::new(&provider)),
    )
}

fn errors(exporter: &InMemoryLogExporter) -> Vec<(Option<AnyValue>, String)> {
    exporter
        .get_emitted_logs()
        .unwrap()
        .iter()
        .filter(|x| x.record.severity_number() == Some(Severity::Error))
        .map(|x| {
            (
                x.record.body().cloned(),
                format!("{:?}", x.record.attributes_iter().collect::<Vec<_>>()),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_failing_use_case_behind_the_api_returns_the_internal_error_problem_and_logs_once() {
    let exporter = InMemoryLogExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(USER_NAME, USER_PASSWORD).await;
    database
        .query("ALTER TABLE users RENAME TO users_gone")
        .await;
    let _tracing = capture_logs(&exporter);

    let (status, body) = server.sign_in(USER_NAME, USER_PASSWORD, "web").await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        body,
        json!({
            "type": "urn:selfless:problem:internal-error",
            "title": "Something went wrong on the server.",
            "status": 500,
        })
    );
    let errors = errors(&exporter);
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (message, attributes) = &errors[0];
    assert_eq!(message, &Some(AnyValue::from("request failed")));
    assert!(attributes.contains("users"), "{attributes}");
    assert!(!attributes.contains(USER_NAME), "{attributes}");
    assert!(!attributes.contains(USER_PASSWORD), "{attributes}");
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_failing_use_case_behind_an_admin_page_returns_a_500_and_logs_once() {
    let exporter = InMemoryLogExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    database
        .query("ALTER TABLE admins RENAME TO admins_gone")
        .await;
    let _tracing = capture_logs(&exporter);

    let page = server.browser().sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    assert_eq!(page.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(page.body, "internal server error");
    let errors = errors(&exporter);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, Some(AnyValue::from("request failed")));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn errors_that_are_not_server_failures_and_web_app_pages_log_nothing() {
    let exporter = InMemoryLogExporter::default();
    let database = TestDatabase::create().await;
    let web_dir = std::env::temp_dir().join(format!("selfless-web-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&web_dir).unwrap();
    std::fs::write(web_dir.join("index.html"), "<title>selfless</title>").unwrap();
    let mut settings = support::settings(database.url());
    settings.web.dir = web_dir.clone();
    let server = TestServer::start_with(settings, FakeClock::new()).await;
    let _tracing = capture_logs(&exporter);
    let mut browser = server.browser();

    let (settings_page, _, _) = server.get_text("/settings").await;
    let redirect = browser.get("/admin").await;
    let forbidden = browser
        .post_form(
            "/admin/sign-in",
            &[
                ("csrf", "wrong"),
                ("name", ADMIN_NAME),
                ("password", ADMIN_PASSWORD),
            ],
        )
        .await;
    let not_found = browser.get("/api/nothing/v1").await;
    let (bad_request, problem) = server
        .request(Method::POST, "/api/sessions/v1", None, Some(json!({})))
        .await;

    assert_eq!(settings_page, StatusCode::OK);
    assert_eq!(redirect.status, StatusCode::SEE_OTHER);
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    assert_eq!(not_found.status, StatusCode::NOT_FOUND);
    assert_eq!(bad_request, StatusCode::BAD_REQUEST);
    assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    assert_eq!(errors(&exporter), []);
    drop(server);
    std::fs::remove_dir_all(web_dir).unwrap();
    database.drop().await;
}
