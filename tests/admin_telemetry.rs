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
