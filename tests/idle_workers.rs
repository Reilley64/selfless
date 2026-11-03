#[path = "../test/selfless.rs"]
pub mod selfless_test;

use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use selfless_test::eventually;
use std::time::Duration;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

#[tokio::test]
async fn idle_workers_export_no_span_after_the_startup_reconciles() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let _tracing = test_fixtures::set_default_subscriber(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
    );
    let database = TestDatabase::create().await;
    let server = selfless_test::start(
        selfless_test::settings(database.url()),
        &FakeClock::new(),
        &FakeMailer::default(),
    )
    .await;
    let started = eventually(async || {
        let spans = exporter.get_finished_spans().unwrap();
        let reconciles = spans
            .iter()
            .filter(|x| x.name.starts_with("reconcile "))
            .count();
        (reconciles == 2).then_some(spans.len())
    })
    .await;

    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(exporter.get_finished_spans().unwrap().len(), started);
    drop(server);
    database.drop().await;
}
