mod support;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME;
use support::USER_PASSWORD;

#[tokio::test]
async fn user_sign_ins_are_counted_by_outcome_and_client_kind() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(USER_NAME, USER_PASSWORD).await;

    server.sign_in(USER_NAME, USER_PASSWORD, "web").await;
    server.sign_in(USER_NAME, USER_PASSWORD, "appleTv").await;
    server.sign_in(USER_NAME, USER_PASSWORD, "googleTv").await;
    server.sign_in(USER_NAME, "wrong", "googleTv").await;
    server.sign_in("Maya", USER_PASSWORD, "googleTv").await;
    provider.force_flush().unwrap();

    let metrics = exporter.get_finished_metrics().unwrap();
    let sign_ins = metrics
        .iter()
        .flat_map(|x| x.scope_metrics())
        .flat_map(|x| x.metrics())
        .find(|x| x.name() == "selfless.auth.sign_ins")
        .expect("the selfless.auth.sign_ins counter");
    let AggregatedMetrics::U64(MetricData::Sum(sum)) = sign_ins.data() else {
        panic!("selfless.auth.sign_ins is a u64 counter");
    };
    let count = |outcome: &'static str, client_kind: &'static str| {
        sum.data_points()
            .find(|x| {
                let attributes = x.attributes().cloned().collect::<Vec<_>>();
                attributes.len() == 2
                    && attributes.contains(&KeyValue::new("selfless.auth.outcome", outcome))
                    && attributes.contains(&KeyValue::new("selfless.auth.client_kind", client_kind))
            })
            .map(|x| x.value())
    };
    assert_eq!(count("succeeded", "web"), Some(1));
    assert_eq!(count("succeeded", "appleTv"), Some(1));
    assert_eq!(count("succeeded", "googleTv"), Some(1));
    assert_eq!(count("failed", "googleTv"), Some(2));
    drop(server);
    database.drop().await;
}
