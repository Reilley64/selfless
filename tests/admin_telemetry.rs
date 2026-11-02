mod support;

use opentelemetry::logs::AnyValue;
use opentelemetry::logs::Severity;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use support::TestDatabase;
use support::TestServer;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

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
        .filter(|x| x.record.severity_number() == Some(Severity::Warn))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    let record = &warnings[0].record;
    assert_eq!(record.body(), Some(&AnyValue::from("admin sign-in failed")));
    let attributes = format!("{:?}", record.attributes_iter().collect::<Vec<_>>());
    assert!(!attributes.contains("Noor"), "{attributes}");
    drop(server);
    database.drop().await;
}
