use application::playback::SubtitleChoice;
use application::playback::start_playback;
use application::ports::Clock;
use application::ports::EndHlsOutput;
use application::ports::ReadKeyframes;
use application::ports::StartHlsOutput;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::ports::persistence::DbScope;
use application::url_signer::UrlSigner;
use domain::CapabilityProfile;
use domain::ClientKind;
use domain::HdrType;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use std::time::Duration;
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

async fn add_movie_file(db_context: &mut DbContext, id: i64, path: &str, container: &str) -> Uuid {
    let media_file = db_context
        .media_files()
        .create()
        .path(path)
        .container(Some(container.to_owned()))
        .duration_ms(Some(60_000))
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    media_file
        .video_streams()
        .create()
        .index(0)
        .codec("h264")
        .profile(Some("High".to_owned()))
        .width(1920)
        .height(800)
        .hdr_type(HdrType::Sdr)
        .interlaced(false)
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    db_context
        .movies()
        .create()
        .tmdb_id(domain::TmdbId(id))
        .radarr_id(domain::RadarrId(id))
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

fn dependencies(db_context: &DbContext) -> start_playback::Dependencies {
    start_playback::Dependencies {
        db_context: db_context.clone(),
        clock: Clock::new(FakeClock::new().reader()),
        url_signer: UrlSigner::new(b"a household secret"),
        read_keyframes: ReadKeyframes::new(|_| {
            Box::pin(async { Ok((0..10).map(|x| Duration::from_secs(x * 6)).collect()) })
        }),
        start_hls_output: StartHlsOutput::new(|_| Box::pin(async { Ok(()) })),
        end_hls_output: EndHlsOutput::new(|_| Box::pin(async {})),
    }
}

#[tokio::test]
async fn play_requests_are_counted_by_play_method_client_kind_and_reason_kinds() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let mut db_context = db_context().await;
    let mp4 = add_movie_file(
        &mut db_context,
        1,
        "/mnt/media/movies/Arrival (2016)/Arrival (2016).mp4",
        "mov,mp4,m4a,3gp,3g2,mj2",
    )
    .await;
    let mkv = add_movie_file(
        &mut db_context,
        2,
        "/mnt/media/movies/Dune (2021)/Dune (2021).mkv",
        "matroska,webm",
    )
    .await;
    let profile = CapabilityProfile {
        containers: vec!["mp4".to_owned()],
        video_codecs: vec![domain::VideoCodecSupport {
            codec: "h264".to_owned(),
            profiles: vec!["high".to_owned()],
            max_level: 51,
            max_width: 1920,
            max_height: 1080,
            bit_depths: vec![8],
            hdr_types: vec![HdrType::Sdr],
            dolby_vision_profiles: Vec::new(),
        }],
        ..CapabilityProfile::default()
    };

    for (client_kind, media_file_id, force_hls) in [
        (ClientKind::Web, mp4, false),
        (ClientKind::Web, mp4, false),
        (ClientKind::GoogleTv, mp4, false),
        (ClientKind::Web, mkv, false),
        (ClientKind::Web, mp4, true),
    ] {
        start_playback::call(
            dependencies(&db_context),
            Uuid::new_v4(),
            Uuid::new_v4(),
            client_kind,
            media_file_id,
            0.0,
            None,
            SubtitleChoice::Default,
            &profile,
            force_hls,
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
    let decision = |client_kind: &str, method: &str, reason_kinds: Option<&str>| {
        let mut attributes = vec![
            (
                "selfless.auth.client_kind".to_owned(),
                client_kind.to_owned(),
            ),
            ("selfless.playback.method".to_owned(), method.to_owned()),
        ];
        attributes.extend(
            reason_kinds.map(|x| ("selfless.playback.reason_kinds".to_owned(), x.to_owned())),
        );
        attributes
    };
    assert_eq!(
        counts,
        [
            (decision("googleTv", "directPlay", None), 1),
            (decision("web", "directPlay", None), 2),
            (decision("web", "remux", Some("[\"container\"]")), 1),
            (decision("web", "remux", Some("[\"forcedHls\"]")), 1),
        ]
    );
}
