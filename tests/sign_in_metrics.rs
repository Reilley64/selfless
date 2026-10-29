mod support;

use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::TestDatabase;
use support::TestServer;

#[tokio::test]
async fn admin_sign_ins_are_counted_by_outcome() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    browser.sign_in("Noor", "wrong").await;
    browser.sign_in(ADMIN_NAME, "wrong").await;
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
    let count = |outcome: &'static str| {
        sum.data_points()
            .find(|x| {
                let attributes = x.attributes().cloned().collect::<Vec<_>>();
                attributes.len() == 2
                    && attributes.contains(&KeyValue::new("selfless.auth.outcome", outcome))
                    && attributes.contains(&KeyValue::new("selfless.auth.client_kind", "admin"))
            })
            .map(|x| x.value())
    };
    assert_eq!(count("succeeded"), Some(1));
    assert_eq!(count("failed"), Some(2));
    drop(server);
    database.drop().await;
}
