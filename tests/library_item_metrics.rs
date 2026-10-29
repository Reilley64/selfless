mod support;

use domain::Source;
use jiff::SignedDuration;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use reqwest::StatusCode;
use serde_json::Value;
use support::FakeClock;
use support::MediaFolder;
use support::StubRadarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::settings;

async fn radarr_lists(radarr: &StubRadarr, media: &MediaFolder, ids: &[i64]) -> Vec<String> {
    let movies = serde_json::from_str::<Vec<Value>>(&media.fixture("radarr/api/movies.json"))
        .unwrap()
        .into_iter()
        .filter(|x| ids.contains(&x["id"].as_i64().unwrap()))
        .collect::<Vec<_>>();
    radarr
        .serve_movies(StatusCode::OK, &Value::Array(movies.clone()).to_string())
        .await;
    movies
        .iter()
        .map(|x| x["movieFile"]["path"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn reconciles_count_movies_added_changed_and_removed_by_source() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let radarr = StubRadarr::start().await;
    let media = MediaFolder::new();
    let files = radarr_lists(&radarr, &media, &[42, 43]).await;
    for file in &files {
        media.create_absolute(file);
    }
    let mut settings = settings(database.url());
    settings.radarr.url = Some(radarr.url());
    let server = TestServer::start_with(settings, FakeClock::new()).await;
    let reconciled = async || {
        let started_at = server.clock.now();
        eventually(async || {
            application::sync_status::call(&server.state, None, 1)
                .await
                .unwrap()
                .radarr
                .last_reconcile
                .filter(|x| x.started_at == started_at)
        })
        .await;
    };
    let sync_now = async || {
        server.clock.advance(SignedDuration::from_mins(1));
        application::sync_now::call(&server.state);
        reconciled().await;
    };
    let count = |name: &'static str| {
        provider.force_flush().unwrap();
        exporter
            .get_finished_metrics()
            .unwrap()
            .iter()
            .flat_map(|x| x.scope_metrics())
            .flat_map(|x| x.metrics())
            .filter(|x| x.name() == name)
            .filter_map(|x| match x.data() {
                AggregatedMetrics::U64(MetricData::Sum(sum)) => sum
                    .data_points()
                    .find(|x| {
                        x.attributes().cloned().collect::<Vec<_>>()
                            == [KeyValue::new(
                                "selfless.library.source",
                                Source::Radarr.name(),
                            )]
                    })
                    .map(|x| x.value()),
                _ => None,
            })
            .last()
    };

    reconciled().await;
    assert_eq!(count("selfless.library.items.added"), Some(2));
    sync_now().await;
    assert_eq!(count("selfless.library.items.changed"), None);
    media.change_absolute(&files[0]);
    sync_now().await;
    assert_eq!(count("selfless.library.items.changed"), Some(1));
    radarr_lists(&radarr, &media, &[42]).await;
    sync_now().await;
    assert_eq!(count("selfless.library.items.removed"), Some(1));
    assert_eq!(count("selfless.library.items.changed"), Some(1));
    drop(server);
    database.drop().await;
}
