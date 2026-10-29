mod support;

use opentelemetry::KeyValue;
use opentelemetry::trace::SpanId;
use opentelemetry::trace::SpanKind;
use opentelemetry::trace::TraceId;
use opentelemetry::trace::TracerProvider;
use opentelemetry_sdk::trace::InMemorySpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::trace::SpanData;
use std::collections::HashSet;
use support::RADARR_WEBHOOK_PASSWORD;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;

const TRACE_ID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const PARENT_SPAN_ID: &str = "00f067aa0ba902b7";

#[tokio::test]
async fn readiness_with_a_traceparent_exports_an_http_span_with_a_child_database_span() {
    let exporter = InMemorySpanExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let _tracing = trace_into(&exporter);
    let reconciles = startup_reconciles(&exporter).await;

    server
        .get_with_traceparent(
            "/actuator/health/readiness/v1",
            &format!("00-{TRACE_ID}-{PARENT_SPAN_ID}-01"),
        )
        .await;

    let spans = spans_outside(&exporter, &reconciles);
    let http = spans
        .iter()
        .find(|x| x.span_kind == SpanKind::Server)
        .expect("an HTTP server span");
    assert_eq!(http.name, "GET /actuator/health/readiness/v1");
    assert_eq!(
        http.span_context.trace_id(),
        TraceId::from_hex(TRACE_ID).unwrap()
    );
    assert_eq!(
        http.parent_span_id,
        SpanId::from_hex(PARENT_SPAN_ID).unwrap()
    );
    assert!(spans.iter().any(|x| {
        x.span_kind == SpanKind::Client
            && x.parent_span_id == http.span_context.span_id()
            && x.attributes
                .contains(&KeyValue::new("db.system.name", "postgresql"))
    }));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn liveness_probe_exports_no_span() {
    let exporter = InMemorySpanExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let _tracing = trace_into(&exporter);
    let reconciles = startup_reconciles(&exporter).await;

    server.get("/actuator/health/liveness/v1").await;

    assert!(spans_outside(&exporter, &reconciles).is_empty());
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn readiness_probe_without_a_traceparent_exports_no_span() {
    let exporter = InMemorySpanExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let _tracing = trace_into(&exporter);
    let reconciles = startup_reconciles(&exporter).await;

    server.get("/actuator/health/readiness/v1").await;

    assert!(spans_outside(&exporter, &reconciles).is_empty());
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_inbox_worker_links_its_span_to_the_webhook_request() {
    let exporter = InMemorySpanExporter::default();
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let _tracing = trace_into(&exporter);

    reqwest::Client::new()
        .post(format!("{}/webhooks/radarr/v1", server.base_url()))
        .basic_auth("radarr", Some(RADARR_WEBHOOK_PASSWORD))
        .header("traceparent", format!("00-{TRACE_ID}-{PARENT_SPAN_ID}-01"))
        .body(fixture("radarr/webhooks/test.json"))
        .send()
        .await
        .unwrap();

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
    assert_eq!(webhook.name, "POST /webhooks/radarr/v1");
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

async fn startup_reconciles(exporter: &InMemorySpanExporter) -> HashSet<TraceId> {
    eventually(async || {
        let traces = exporter
            .get_finished_spans()
            .unwrap()
            .into_iter()
            .filter(|x| x.name.starts_with("reconcile "))
            .map(|x| x.span_context.trace_id())
            .collect::<HashSet<_>>();
        (traces.len() == 2).then_some(traces)
    })
    .await
}

fn spans_outside(exporter: &InMemorySpanExporter, traces: &HashSet<TraceId>) -> Vec<SpanData> {
    exporter
        .get_finished_spans()
        .unwrap()
        .into_iter()
        .filter(|x| !traces.contains(&x.span_context.trace_id()))
        .collect()
}

fn trace_into(exporter: &InMemorySpanExporter) -> DefaultGuard {
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
    )
}
