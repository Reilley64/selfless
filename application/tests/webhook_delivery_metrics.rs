use application::library::Prober;
use application::library::SourceHealth;
use application::ports::Clock;
use application::ports::FindMediaFile;
use application::ports::GetRadarrMovie;
use application::ports::GetSonarrShow;
use application::ports::ListSubtitleFiles;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::RadarrError;
use application::ports::RadarrWebhook;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadRadarrWebhook;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::ReadSonarrWebhook;
use application::ports::ReadWebhookEventType;
use application::ports::SonarrWebhook;
use application::ports::UnreadableNfo;
use application::ports::UnreadableWebhook;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::webhooks::WebhookPasswords;
use application::webhooks::process_radarr_delivery;
use application::webhooks::process_sonarr_delivery;
use application::webhooks::store_webhook_delivery;
use bytes::Bytes;
use domain::Source;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use test_fixtures::FakeClock;
use uuid::Uuid;

const PASSWORD: &str = "webhook password";

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

fn event_type(event_type: &'static str) -> ReadWebhookEventType {
    ReadWebhookEventType::new(move |_| Some(event_type.to_owned()))
}

async fn store(db_context: &DbContext, source: Source, password: &str, event: &'static str) {
    store_webhook_delivery::call(
        store_webhook_delivery::Dependencies {
            clock: Clock::new(FakeClock::new().reader()),
            db_context: db_context.clone(),
            read_webhook_event_type: event_type(event),
            webhook_passwords: WebhookPasswords {
                radarr: PASSWORD.to_owned(),
                sonarr: PASSWORD.to_owned(),
            },
        },
        source,
        Some(password.to_owned()),
        Bytes::new(),
    )
    .await
    .unwrap();
}

async fn process_radarr(
    db_context: &DbContext,
    read_radarr_webhook: ReadRadarrWebhook,
    event: &'static str,
) -> Result<process_radarr_delivery::Output, process_radarr_delivery::Error> {
    process_radarr_delivery::call(
        process_radarr_delivery::Dependencies {
            db_context: db_context.clone(),
            find_media_file: FindMediaFile::new(|_| Box::pin(async { None })),
            get_radarr_movie: GetRadarrMovie::new(|radarr_id| {
                Box::pin(async move {
                    match radarr_id {
                        1 => Ok(None),
                        _ => Err(RadarrError::caused_by("Radarr is down")),
                    }
                })
            }),
            list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
            prober: Prober::new(ProbeMediaFile::new(|_| {
                Box::pin(async { Err(ProbeError::default()) })
            })),
            read_movie_folder: ReadMovieFolder::new(|_| Box::pin(async { Default::default() })),
            read_movie_nfo: ReadMovieNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default()) })
            }),
            read_radarr_webhook,
            read_webhook_event_type: event_type(event),
            source_health: SourceHealth::default(),
        },
        Uuid::nil(),
        Vec::new(),
    )
    .await
}

async fn process_sonarr(
    db_context: &DbContext,
    read_sonarr_webhook: ReadSonarrWebhook,
    event: &'static str,
) {
    process_sonarr_delivery::call(
        process_sonarr_delivery::Dependencies {
            db_context: db_context.clone(),
            find_media_file: FindMediaFile::new(|_| Box::pin(async { None })),
            get_sonarr_show: GetSonarrShow::new(|_| Box::pin(async { Ok(None) })),
            list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
            prober: Prober::new(ProbeMediaFile::new(|_| {
                Box::pin(async { Err(ProbeError::default()) })
            })),
            read_episode_folder: ReadEpisodeFolder::new(|_| Box::pin(async { Default::default() })),
            read_episode_nfo: ReadEpisodeNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default()) })
            }),
            read_show_folder: ReadShowFolder::new(|_, _| Box::pin(async { Default::default() })),
            read_show_nfo: ReadShowNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) })),
            read_sonarr_webhook,
            read_webhook_event_type: event_type(event),
            source_health: SourceHealth::default(),
        },
        Uuid::nil(),
        Vec::new(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn deliveries_are_counted_as_stored_rejected_processed_ignored_skipped_and_failed_by_source_and_event_type()
 {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let db_context = db_context().await;

    store(&db_context, Source::Radarr, PASSWORD, "Download").await;
    store(&db_context, Source::Radarr, "wrong password", "Download").await;
    store(&db_context, Source::Sonarr, PASSWORD, "Test").await;
    process_radarr(
        &db_context,
        ReadRadarrWebhook::new(|_| Ok(RadarrWebhook::Movie { radarr_id: 1 })),
        "MovieDelete",
    )
    .await
    .unwrap();
    process_radarr(
        &db_context,
        ReadRadarrWebhook::new(|_| Ok(RadarrWebhook::Ignored)),
        "Grab",
    )
    .await
    .unwrap();
    process_radarr(
        &db_context,
        ReadRadarrWebhook::new(|_| Err(UnreadableWebhook::caused_by("missing field `movie`"))),
        "Download",
    )
    .await
    .unwrap();
    process_radarr(
        &db_context,
        ReadRadarrWebhook::new(|_| Ok(RadarrWebhook::Movie { radarr_id: 42 })),
        "Download",
    )
    .await
    .unwrap_err();
    process_sonarr(
        &db_context,
        ReadSonarrWebhook::new(|_| Ok(SonarrWebhook::Show { sonarr_id: 7 })),
        "Download",
    )
    .await;
    process_sonarr(
        &db_context,
        ReadSonarrWebhook::new(|_| Ok(SonarrWebhook::Ignored)),
        "Test",
    )
    .await;
    provider.force_flush().unwrap();

    let metrics = exporter.get_finished_metrics().unwrap();
    let deliveries = metrics
        .iter()
        .flat_map(|x| x.scope_metrics())
        .flat_map(|x| x.metrics())
        .find(|x| x.name() == "selfless.webhook.deliveries")
        .expect("the selfless.webhook.deliveries counter");
    let AggregatedMetrics::U64(MetricData::Sum(sum)) = deliveries.data() else {
        panic!("selfless.webhook.deliveries is a u64 counter");
    };
    let mut counts = sum
        .data_points()
        .map(|x| {
            let attribute = |key: &str| {
                x.attributes()
                    .find(|x| x.key.as_str() == key)
                    .map(|x| x.value.to_string())
            };
            (
                attribute("selfless.webhook.source").unwrap(),
                attribute("selfless.webhook.outcome").unwrap(),
                attribute("selfless.webhook.event_type"),
                x.value(),
            )
        })
        .collect::<Vec<_>>();
    counts.sort();
    let expected = [
        ("radarr", "failed", Some("Download"), 1),
        ("radarr", "ignored", Some("Grab"), 1),
        ("radarr", "processed", Some("MovieDelete"), 1),
        ("radarr", "rejected", None, 1),
        ("radarr", "skipped", Some("Download"), 1),
        ("radarr", "stored", Some("Download"), 1),
        ("sonarr", "ignored", Some("Test"), 1),
        ("sonarr", "processed", Some("Download"), 1),
        ("sonarr", "stored", Some("Test"), 1),
    ]
    .map(|(source, outcome, event_type, count)| {
        (
            source.to_owned(),
            outcome.to_owned(),
            event_type.map(str::to_owned),
            count,
        )
    });
    assert_eq!(counts, expected);
}
