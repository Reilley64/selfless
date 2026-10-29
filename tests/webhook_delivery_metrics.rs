mod support;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use reqwest::Method;
use reqwest::StatusCode;
use support::MediaFolder;
use support::RADARR_WEBHOOK_PASSWORD;
use support::SONARR_WEBHOOK_PASSWORD;
use support::StubSonarr;
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
async fn deliveries_are_counted_as_stored_rejected_processed_and_ignored_by_source_and_event_type()
{
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let sonarr = StubSonarr::start().await;
    let server = TestServer::start_with_sonarr(database.url(), &sonarr).await;

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
    let media = MediaFolder::new();
    sonarr
        .serve_series(
            7,
            &media.sonarr_fixture("sonarr/api/series.json"),
            &media.sonarr_fixture("sonarr/api/episodes.json"),
            &media.sonarr_fixture("sonarr/api/episode-files.json"),
        )
        .await;
    for (_, name) in IGNORED_EVENTS.iter().chain([&("Download", "download")]) {
        let status = server
            .send_sonarr_webhook(
                Method::POST,
                Some(("sonarr", SONARR_WEBHOOK_PASSWORD)),
                &fixture(&format!("sonarr/webhooks/{name}.json")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    let count = |source: &'static str, outcome: &'static str, event_type: Option<&'static str>| {
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
                            KeyValue::new("selfless.webhook.source", source),
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
    for source in ["radarr", "sonarr"] {
        for (event_type, _) in IGNORED_EVENTS {
            eventually(async || {
                (count(source, "ignored", Some(event_type)) == Some(1)).then_some(())
            })
            .await;
            assert_eq!(
                count(source, "stored", Some(event_type)),
                Some(1),
                "{source} {event_type}"
            );
        }
    }
    eventually(async || (count("sonarr", "processed", Some("Download")) == Some(1)).then_some(()))
        .await;
    assert_eq!(count("radarr", "rejected", None), Some(1));
    drop(server);
    database.drop().await;
}
