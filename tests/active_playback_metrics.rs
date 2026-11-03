#[path = "../test/persistence.rs"]
mod persistence_test;
#[path = "../test/selfless.rs"]
pub mod selfless_test;

use application::ports::persistence::DbScope;
use domain::PlayMethod;
use jiff::Timestamp;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use selfless_test::eventually;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use uuid::Uuid;

#[tokio::test]
async fn the_server_records_the_active_playbacks_once_it_starts() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let mut db_context = persistence_test::db_context(&database).await;
    db_context
        .playbacks()
        .create()
        .user_id(Uuid::new_v4())
        .session_id(Uuid::new_v4())
        .media_file_id(Uuid::new_v4())
        .method(PlayMethod::DirectPlay)
        .position(0.0)
        .paused(false)
        .started_at(Timestamp::UNIX_EPOCH)
        .last_reported_at(Timestamp::UNIX_EPOCH)
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    drop(db_context);

    let server = selfless_test::start(
        selfless_test::settings(database.url()),
        &FakeClock::new(),
        &FakeMailer::default(),
    )
    .await;

    let values = eventually(async || {
        provider.force_flush().unwrap();
        let metrics = exporter.get_finished_metrics().unwrap();
        let active = metrics
            .iter()
            .flat_map(|x| x.scope_metrics())
            .flat_map(|x| x.metrics())
            .find(|x| x.name() == "selfless.playback.active")?;
        let AggregatedMetrics::I64(MetricData::Gauge(gauge)) = active.data() else {
            panic!("selfless.playback.active is an i64 gauge");
        };
        let mut values = gauge
            .data_points()
            .map(|x| {
                let method = x
                    .attributes()
                    .find(|x| x.key.as_str() == "selfless.playback.method")
                    .map(|x| x.value.to_string())
                    .unwrap();
                (method, x.value())
            })
            .collect::<Vec<_>>();
        values.sort();
        Some(values)
    })
    .await;
    assert_eq!(
        values,
        [
            ("directPlay".to_owned(), 1),
            ("remux".to_owned(), 0),
            ("transcode".to_owned(), 0),
        ]
    );
    drop(server);
    database.drop().await;
}
