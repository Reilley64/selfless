mod support;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use reqwest::Method;
use reqwest::StatusCode;
use support::RADARR_WEBHOOK_PASSWORD;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;

const IGNORED_EVENTS: [(&str, &str); 6] = [
    ("Test", "test"),
    ("Grab", "grab"),
    ("Health", "health"),
    ("HealthRestored", "health-restored"),
    ("ApplicationUpdate", "application-update"),
    ("ManualInteractionRequired", "manual-interaction-required"),
];

#[tokio::test]
async fn radarr_deliveries_are_counted_as_stored_rejected_and_ignored_by_event_type() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    for (_, name) in IGNORED_EVENTS {
        let status = server
            .send_radarr_webhook(
                Method::POST,
                Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
                &fixture(&format!("radarr/webhooks/{name}.json")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
    let rejected = server
        .send_radarr_webhook(
            Method::POST,
            Some(("radarr", "wrong password")),
            &fixture("radarr/webhooks/grab.json"),
        )
        .await;
    assert_eq!(rejected, StatusCode::UNAUTHORIZED);

    let count = |outcome: &'static str, event_type: Option<&'static str>| {
        provider.force_flush().unwrap();
        exporter
            .get_finished_metrics()
            .unwrap()
            .iter()
            .flat_map(|x| x.scope_metrics())
            .flat_map(|x| x.metrics())
            .filter(|x| x.name() == "selfless.webhook.deliveries")
            .filter_map(|x| match x.data() {
                AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
                    .data_points()
                    .find(|x| {
                        let mut attributes = x.attributes().cloned().collect::<Vec<_>>();
                        attributes.sort_by(|a, b| a.key.cmp(&b.key));
                        let mut expected = vec![
                            KeyValue::new("selfless.webhook.source", "radarr"),
                            KeyValue::new("selfless.webhook.outcome", outcome),
                        ];
                        expected.extend(
                            event_type.map(|x| KeyValue::new("selfless.webhook.event_type", x)),
                        );
                        expected.sort_by(|a, b| a.key.cmp(&b.key));
                        attributes == expected
                    })
                    .map(|x| x.value()),
                _ => None,
            })
            .last()
    };
    for (event_type, _) in IGNORED_EVENTS {
        eventually(async || (count("ignored", Some(event_type)) == Some(1)).then_some(())).await;
        assert_eq!(count("stored", Some(event_type)), Some(1), "{event_type}");
    }
    assert_eq!(count("rejected", None), Some(1));
    drop(server);
    database.drop().await;
}
