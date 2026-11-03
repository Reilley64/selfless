use application::browsing::Resizer;
use application::browsing::get_image;
use application::browsing::list_movies;
use application::ports::Clock;
use application::ports::ImageFile;
use application::ports::ImageFormat;
use application::ports::ReadCachedImage;
use application::ports::ResizeImage;
use application::ports::SaveCachedImage;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::ports::persistence::DbScope;
use application::url_signer::UrlSigner;
use domain::ArtworkKind;
use domain::GridSort;
use opentelemetry::global;
use opentelemetry_sdk::metrics::InMemoryMetricExporter;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::metrics::data::AggregatedMetrics;
use opentelemetry_sdk::metrics::data::MetricData;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use test_fixtures::FakeClock;

#[tokio::test]
async fn image_requests_are_counted_by_whether_the_cache_held_the_image() {
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
    let movie = db_context
        .movies()
        .create()
        .tmdb_id(domain::TmdbId(329865))
        .radarr_id(domain::RadarrId(42))
        .title("Arrival")
        .year(2016)
        .genres(Vec::<String>::new())
        .directors(Vec::<String>::new())
        .in_library(true)
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    db_context
        .artwork()
        .create()
        .movie_id(Some(movie.id))
        .kind(ArtworkKind::Fanart)
        .path("/mnt/media/movies/Arrival (2016)/fanart.jpg")
        .modified_at(jiff::Timestamp::UNIX_EPOCH)
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    let clock = Clock::new(FakeClock::new().reader());
    let url_signer = UrlSigner::new(b"a household secret");
    let cache = Arc::new(Mutex::new(HashMap::new()));
    let fanart = list_movies::call(
        list_movies::Dependencies {
            db_context: db_context.clone(),
            clock: clock.clone(),
            url_signer: url_signer.clone(),
        },
        None,
        GridSort::Title,
        None,
    )
    .await
    .unwrap()
    .tiles
    .remove(0)
    .images
    .fanart
    .unwrap();
    let read_cached_image = ReadCachedImage::new({
        let cache = cache.clone();
        move |image| {
            let file = cache.lock().unwrap().get(&image).cloned();
            Box::pin(async move { Ok(file) })
        }
    });
    let save_cached_image = SaveCachedImage::new(move |image, file| {
        cache.lock().unwrap().insert(image, file);
        Box::pin(async { Ok(()) })
    });
    let resizer = Resizer::new(ResizeImage::new(|_, _| {
        Box::pin(async {
            Ok(ImageFile {
                format: ImageFormat::Jpeg,
                bytes: "fanart".into(),
            })
        })
    }));
    let dependencies = || get_image::Dependencies {
        db_context: db_context.clone(),
        clock: clock.clone(),
        url_signer: url_signer.clone(),
        read_cached_image: read_cached_image.clone(),
        resizer: resizer.clone(),
        save_cached_image: save_cached_image.clone(),
    };

    for url in [&fanart.urls[0], &fanart.urls[0], &fanart.urls[1]] {
        get_image::call(
            dependencies(),
            fanart.id,
            url.width,
            url.expires,
            &url.signature,
        )
        .await
        .unwrap();
    }
    get_image::call(
        dependencies(),
        fanart.id,
        480,
        fanart.urls[0].expires,
        "tampered",
    )
    .await
    .unwrap();
    provider.force_flush().unwrap();

    let metrics = exporter.get_finished_metrics().unwrap();
    let requests = metrics
        .iter()
        .flat_map(|x| x.scope_metrics())
        .flat_map(|x| x.metrics())
        .find(|x| x.name() == "selfless.images.requests")
        .expect("the selfless.images.requests counter");
    let AggregatedMetrics::U64(MetricData::Sum(sum)) = requests.data() else {
        panic!("selfless.images.requests is a u64 counter");
    };
    let mut counts = sum
        .data_points()
        .map(|x| {
            let attributes = x
                .attributes()
                .map(|x| (x.key.to_string(), x.value.to_string()))
                .collect::<Vec<_>>();
            (attributes, x.value())
        })
        .collect::<Vec<_>>();
    counts.sort();
    let cache = |outcome: &str| vec![("selfless.images.cache".to_owned(), outcome.to_owned())];
    assert_eq!(counts, [(cache("hit"), 1), (cache("resized"), 2)]);
}
