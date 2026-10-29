mod support;

use opentelemetry::KeyValue;
use opentelemetry::logs::AnyValue;
use opentelemetry::logs::Severity;
use opentelemetry::trace::SpanKind;
use opentelemetry::trace::TracerProvider;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use reqwest::Method;
use support::TestDatabase;
use support::TestServer;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

#[tokio::test]
async fn authenticated_request_spans_carry_the_user_id() {
    let exporter = InMemorySpanExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let token = server.signed_in_user().await;
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
    );

    let (_, me) = server
        .request(Method::GET, "/api/me/v1", Some(&token), None)
        .await;

    let spans = exporter.get_finished_spans().unwrap();
    let http = spans
        .iter()
        .find(|x| x.span_kind == SpanKind::Server)
        .expect("an HTTP server span");
    assert_eq!(http.name, "GET /api/me/v1");
    assert!(
        http.attributes.contains(&KeyValue::new(
            "user.id",
            me["id"].as_str().unwrap().to_owned()
        )),
        "{:?}",
        http.attributes
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_failed_user_sign_in_logs_a_warning_without_the_name() {
    let exporter = InMemoryLogExporter::default();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let _tracing = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(OpenTelemetryTracingBridge::new(&provider)),
    );

    server.sign_in("Maya", "wrong", "web").await;

    let logs = exporter.get_emitted_logs().unwrap();
    let warnings = logs
        .iter()
        .filter(|x| {
            x.record.severity_number() == Some(Severity::Warn)
                && x.record.body() == Some(&AnyValue::from("user sign-in failed"))
        })
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    let record = &warnings[0].record;
    let attributes = format!("{:?}", record.attributes_iter().collect::<Vec<_>>());
    assert!(!attributes.contains("Maya"), "{attributes}");
    drop(server);
    database.drop().await;
}
