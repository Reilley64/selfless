use application::library::Prober;
use application::library::SourceHealth;
use application::library::sync_movie;
use application::ports::FindMediaFile;
use application::ports::FoundMediaFile;
use application::ports::GetRadarrMovie;
use application::ports::ListSubtitleFiles;
use application::ports::Probe;
use application::ports::ProbeMediaFile;
use application::ports::RadarrMovie;
use application::ports::RadarrMovieFile;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::UnreadableNfo;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use jiff::Timestamp;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use std::sync::Arc;
use std::sync::Mutex;

const FILE: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";

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

fn at(timestamp: &str) -> Timestamp {
    timestamp.parse().expect("the timestamp is valid")
}

fn arrival() -> RadarrMovie {
    RadarrMovie {
        radarr_id: 42,
        tmdb_id: 329_865,
        title: "Arrival".to_owned(),
        year: 2016,
        overview: None,
        genres: Vec::new(),
        movie_file: Some(RadarrMovieFile {
            path: FILE.to_owned(),
            date_added: at("2026-10-29T10:00:00Z"),
        }),
    }
}

#[tokio::test]
async fn movie_syncs_count_movies_added_changed_and_removed_by_source() {
    let exporter = InMemoryMetricExporter::default();
    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter.clone())
        .build();
    global::set_meter_provider(provider.clone());
    let db_context = db_context().await;
    let radarr_movie = Arc::new(Mutex::new(Some(arrival())));
    let file_on_disk = Arc::new(Mutex::new(FoundMediaFile {
        size: 1_000,
        modified_at: at("2026-10-29T09:00:00Z"),
    }));
    let sync = async || {
        let radarr_movie = radarr_movie.clone();
        let file_on_disk = file_on_disk.clone();
        sync_movie::call(
            sync_movie::Dependencies {
                db_context: db_context.clone(),
                find_media_file: FindMediaFile::new(move |_| {
                    let found = *file_on_disk.lock().unwrap();
                    Box::pin(async move { Ok(Some(found)) })
                }),
                get_radarr_movie: GetRadarrMovie::new(move |_| {
                    let movie = radarr_movie.lock().unwrap().clone();
                    Box::pin(async move { Ok(movie) })
                }),
                list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
                prober: Prober::new(ProbeMediaFile::new(|_| {
                    Box::pin(async {
                        Ok(Probe {
                            container: "matroska,webm".to_owned(),
                            duration: None,
                            video_streams: Vec::new(),
                            audio_streams: Vec::new(),
                            subtitle_streams: Vec::new(),
                        })
                    })
                })),
                read_movie_folder: ReadMovieFolder::new(|_| {
                    Box::pin(async { Ok(Default::default()) })
                }),
                read_movie_nfo: ReadMovieNfo::new(|_| {
                    Box::pin(async { Err(UnreadableNfo::default()) })
                }),
                source_health: SourceHealth::default(),
            },
            42,
        )
        .await
        .unwrap();
    };
    let count = |name: &str| {
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
                            == [KeyValue::new("selfless.library.source", "radarr")]
                    })
                    .map(|x| x.value()),
                _ => None,
            })
            .last()
    };

    sync().await;
    assert_eq!(count("selfless.library.items.added"), Some(1));
    sync().await;
    assert_eq!(count("selfless.library.items.added"), Some(1));
    assert_eq!(count("selfless.library.items.changed"), None);
    file_on_disk.lock().unwrap().size = 2_000;
    sync().await;
    assert_eq!(count("selfless.library.items.changed"), Some(1));
    *radarr_movie.lock().unwrap() = None;
    sync().await;

    assert_eq!(count("selfless.library.items.removed"), Some(1));
    assert_eq!(count("selfless.library.items.changed"), Some(1));
}
