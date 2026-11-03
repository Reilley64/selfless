#[path = "../test/selfless.rs"]
pub mod selfless_test;

use opentelemetry::KeyValue;
use opentelemetry::trace::SpanKind;
use opentelemetry::trace::TraceId;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use selfless_test::RADARR_WEBHOOK_PASSWORD;
use selfless_test::eventually;
use selfless_test::fixture;
use selfless_test::send_webhook;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;

const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const PARENT_SPAN_ID: &str = "00f067aa0ba902b7";

#[tokio::test]
async fn the_inbox_worker_links_its_span_to_the_webhook_request() {
    let exporter = InMemorySpanExporter::default();
    let _tracing = trace_into(&exporter);
    let database = TestDatabase::create().await;
    let server = selfless_test::start(
        selfless_test::settings(database.url()),
        &FakeClock::new(),
        &FakeMailer::default(),
    )
    .await;
    let mut headers = HeaderMap::new();
    headers.insert(
        "traceparent",
        format!("00-{TRACE_ID}-{PARENT_SPAN_ID}-01")
            .parse()
            .unwrap(),
    );

    let status = send_webhook(
        &server,
        "/webhooks/radarr/v1",
        RADARR_WEBHOOK_PASSWORD,
        headers,
        &fixture("radarr/webhooks/test.json"),
    )
    .await;

    let worker = eventually(async || {
        exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .find(|x| x.span_kind == SpanKind::Consumer)
    })
    .await;
    let spans = exporter.get_finished_spans().unwrap();
    let webhook = spans
        .iter()
        .find(|x| x.span_kind == SpanKind::Server)
        .expect("an HTTP server span");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(webhook.name, "POST /webhooks/radarr/v1");
    assert_eq!(worker.name, "process inboxes");
    assert_ne!(
        worker.span_context.trace_id(),
        webhook.span_context.trace_id()
    );
    assert_eq!(
        worker
            .links
            .iter()
            .map(|x| (x.span_context.trace_id(), x.span_context.span_id()))
            .collect::<Vec<_>>(),
        vec![(
            TraceId::from_hex(TRACE_ID).unwrap(),
            webhook.span_context.span_id()
        )]
    );
    assert!(
        worker
            .attributes
            .contains(&KeyValue::new("selfless.webhook.outcome", "ignored"))
    );
    drop(server);
    database.drop().await;
}

fn trace_into(exporter: &InMemorySpanExporter) -> DefaultGuard {
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    test_fixtures::set_default_subscriber(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
    )
}
