use application::admins::create_admin;
use application::ports::Clock;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::sessions::admin_sign_in;
use application::sessions::sign_in;
use application::users::create_user;
use domain::ClientKind;
use domain::EmailAddress;
use domain::Password;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use test_fixtures::FakeClock;
use uuid::Uuid;

const PASSWORD: &str = "a long passphrase";

async fn db_context() -> DbContext {
    DbContext::new(
        test_fixtures::mock_database(persistence::models()).await,
        DbLocation {
            database: "mock".to_owned(),
            address: "localhost".to_owned(),
            port: 0,
        },
    )
}

async fn sign_in(db_context: &DbContext, name: &str, password: &str, client_kind: ClientKind) {
    sign_in::call(
        sign_in::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        name.to_owned(),
        password.to_owned(),
        client_kind,
        "Living Room".to_owned(),
        Uuid::new_v4(),
    )
    .await
    .unwrap();
}

async fn admin_sign_in(db_context: &DbContext, name: &str, password: &str) {
    admin_sign_in::call(
        admin_sign_in::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        name.to_owned(),
        password.to_owned(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn sign_ins_are_counted_by_outcome_and_client_kind() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let db_context = db_context().await;
    create_user::call(
        create_user::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "Noor".to_owned(),
        Password::new(PASSWORD.to_owned()).unwrap(),
        EmailAddress::new("noor@example.com".to_owned()).unwrap(),
    )
    .await
    .unwrap();
    create_admin::call(
        create_admin::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(FakeClock::new().reader()),
        },
        "Maya".to_owned(),
        Password::new(PASSWORD.to_owned()).unwrap(),
    )
    .await
    .unwrap();

    sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await;
    sign_in(&db_context, "Noor", PASSWORD, ClientKind::AppleTv).await;
    sign_in(&db_context, "Noor", PASSWORD, ClientKind::GoogleTv).await;
    sign_in(&db_context, "Noor", "wrong", ClientKind::GoogleTv).await;
    sign_in(&db_context, "Theo", PASSWORD, ClientKind::GoogleTv).await;
    admin_sign_in(&db_context, "Maya", PASSWORD).await;
    admin_sign_in(&db_context, "Maya", "wrong").await;
    admin_sign_in(&db_context, "Theo", PASSWORD).await;
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
    let mut counts = sum
        .data_points()
        .map(|x| {
            let attribute = |key: &str| {
                x.attributes()
                    .find(|x| x.key.as_str() == key)
                    .map(|x| x.value.to_string())
                    .unwrap()
            };
            assert_eq!(x.attributes().count(), 2);
            (
                attribute("selfless.auth.outcome"),
                attribute("selfless.auth.client_kind"),
                x.value(),
            )
        })
        .collect::<Vec<_>>();
    counts.sort();
    let expected = [
        ("failed", "admin", 2),
        ("failed", "googleTv", 2),
        ("succeeded", "admin", 1),
        ("succeeded", "appleTv", 1),
        ("succeeded", "googleTv", 1),
        ("succeeded", "web", 1),
    ]
    .map(|(outcome, client_kind, count)| (outcome.to_owned(), client_kind.to_owned(), count));
    assert_eq!(counts, expected);
}
