use application::playback::start_playback;
use application::ports::Clock;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::ports::persistence::DbScope;
use application::url_signer::UrlSigner;
use domain::CapabilityProfile;
use domain::ClientKind;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use test_fixtures::FakeClock;
use uuid::Uuid;

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

async fn add_movie_file(db_context: &mut DbContext) -> Uuid {
    let media_file = db_context
        .media_files()
        .create()
        .path("/mnt/media/movies/Arrival (2016)/Arrival (2016).mp4")
        .container(Some("mov,mp4,m4a,3gp,3g2,mj2".to_owned()))
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    db_context
        .movies()
        .create()
        .tmdb_id(domain::TmdbId(329865))
        .radarr_id(domain::RadarrId(42))
        .title("Arrival")
        .year(2016)
        .genres(Vec::<String>::new())
        .directors(Vec::<String>::new())
        .in_library(true)
        .media_file_id(Some(media_file.id))
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    media_file.id
}

#[tokio::test]
async fn play_requests_are_counted_by_play_method_and_client_kind() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let mut db_context = db_context().await;
    let media_file_id = add_movie_file(&mut db_context).await;
    let profile = CapabilityProfile {
        containers: vec!["mp4".to_owned()],
        ..CapabilityProfile::default()
    };

    for client_kind in [ClientKind::Web, ClientKind::Web, ClientKind::GoogleTv] {
        start_playback::call(
            start_playback::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
                url_signer: UrlSigner::new(b"a household secret"),
            },
            Uuid::new_v4(),
            Uuid::new_v4(),
            client_kind,
            media_file_id,
            0.0,
            &profile,
        )
        .await
        .unwrap();
    }
    provider.force_flush().unwrap();

    let metrics = exporter.get_finished_metrics().unwrap();
    let decisions = metrics
        .iter()
        .flat_map(|x| x.scope_metrics())
        .flat_map(|x| x.metrics())
        .find(|x| x.name() == "selfless.playback.decisions")
        .expect("the selfless.playback.decisions counter");
    let AggregatedMetrics::U64(MetricData::Sum(sum)) = decisions.data() else {
        panic!("selfless.playback.decisions is a u64 counter");
    };
    let mut counts = sum
        .data_points()
        .map(|x| {
            let mut attributes = x
                .attributes()
                .map(|x| (x.key.to_string(), x.value.to_string()))
                .collect::<Vec<_>>();
            attributes.sort();
            (attributes, x.value())
        })
        .collect::<Vec<_>>();
    counts.sort();
    let decision = |client_kind: &str| {
        vec![
            (
                "selfless.auth.client_kind".to_owned(),
                client_kind.to_owned(),
            ),
            (
                "selfless.playback.method".to_owned(),
                "directPlay".to_owned(),
            ),
        ]
    };
    assert_eq!(counts, [(decision("googleTv"), 1), (decision("web"), 2)]);
}
