use application::playback::record_active_playbacks;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::ports::persistence::DbScope;
use domain::PlayMethod;
use jiff::Timestamp;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use uuid::Uuid;

#[tokio::test]
async fn playbacks_that_have_not_ended_are_recorded_by_play_method() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let mut db_context = DbContext::new(
        test_fixtures::mock_database(persistence::models()).await,
        DbLocation {
            database: "mock".to_owned(),
            address: "localhost".to_owned(),
            port: 0,
        },
    );
    for (method, ended_at) in [
        (PlayMethod::DirectPlay, None),
        (PlayMethod::DirectPlay, None),
        (PlayMethod::DirectPlay, Some(Timestamp::UNIX_EPOCH)),
        (PlayMethod::Transcode, None),
        (PlayMethod::Remux, Some(Timestamp::UNIX_EPOCH)),
    ] {
        db_context
            .playbacks()
            .create()
            .user_id(Uuid::new_v4())
            .session_id(Uuid::new_v4())
            .media_file_id(Uuid::new_v4())
            .method(method)
            .position(0.0)
            .paused(false)
            .started_at(Timestamp::UNIX_EPOCH)
            .last_reported_at(Timestamp::UNIX_EPOCH)
            .ended_at(ended_at)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    record_active_playbacks::call(record_active_playbacks::Dependencies { db_context })
        .await
        .unwrap();
    provider.force_flush().unwrap();

    let metrics = exporter.get_finished_metrics().unwrap();
    let active = metrics
        .iter()
        .flat_map(|x| x.scope_metrics())
        .flat_map(|x| x.metrics())
        .find(|x| x.name() == "selfless.playback.active")
        .expect("the selfless.playback.active gauge");
    let AggregatedMetrics::I64(MetricData::Gauge(gauge)) = active.data() else {
        panic!("selfless.playback.active is an i64 gauge");
    };
    let mut values = gauge
        .data_points()
        .map(|x| {
            let attributes = x
                .attributes()
                .map(|x| (x.key.to_string(), x.value.to_string()))
                .collect::<Vec<_>>();
            (attributes, x.value())
        })
        .collect::<Vec<_>>();
    values.sort();
    let method = |method: &str| vec![("selfless.playback.method".to_owned(), method.to_owned())];
    assert_eq!(
        values,
        [
            (method("directPlay"), 2),
            (method("remux"), 0),
            (method("transcode"), 1),
        ]
    );
}
