#[path = "../test/persistence.rs"]
mod persistence_test;
#[path = "../test/selfless.rs"]
pub mod selfless_test;

use application::ports::persistence::DbScope;
use domain::Episode;
use domain::Movie;
use domain::Show;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;
use selfless_test::MediaFolder;
use selfless_test::RADARR_WEBHOOK_PASSWORD;
use selfless_test::SONARR_WEBHOOK_PASSWORD;
use selfless_test::StubRadarr;
use selfless_test::StubSonarr;
use selfless_test::eventually;
use selfless_test::fixture;
use selfless_test::send_webhook;
use serde_json::Value;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;

#[tokio::test]
async fn a_radarr_download_delivery_re_syncs_its_movie_while_the_server_runs() {
    let database = TestDatabase::create().await;
    let radarr = StubRadarr::start().await;
    let media = MediaFolder::default();
    media.create("Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv");
    radarr
        .serve_movie(42, &media.fixture("radarr/api/movie.json"))
        .await;
    let mut settings = selfless_test::settings(database.url());
    settings.radarr.url = Some(radarr.url());
    let server = selfless_test::start(settings, &FakeClock::new(), &FakeMailer::default()).await;
    let mut db_context = persistence_test::db_context(&database).await;

    let status = send_webhook(
        &server,
        "/webhooks/radarr/v1",
        RADARR_WEBHOOK_PASSWORD,
        HeaderMap::new(),
        &fixture("radarr/webhooks/download.json"),
    )
    .await;

    let movie = eventually(async || {
        db_context
            .movies()
            .filter(Movie::fields().in_library().eq(true))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    })
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(movie.title, "Arrival");
    drop(db_context);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_sonarr_download_delivery_re_syncs_its_show_while_the_server_runs() {
    let database = TestDatabase::create().await;
    let sonarr = StubSonarr::start().await;
    let media = MediaFolder::default();
    let episode_files: Vec<Value> =
        serde_json::from_str(&fixture("sonarr/api/episode-files.json")).unwrap();
    for file in episode_files {
        media.create(
            file["path"]
                .as_str()
                .unwrap()
                .strip_prefix("/mnt/media/")
                .unwrap(),
        );
    }
    sonarr
        .serve_series(
            7,
            &media.fixture("sonarr/api/series.json"),
            &media.fixture("sonarr/api/episodes.json"),
            &media.fixture("sonarr/api/episode-files.json"),
        )
        .await;
    let mut settings = selfless_test::settings(database.url());
    settings.sonarr.url = Some(sonarr.url());
    let server = selfless_test::start(settings, &FakeClock::new(), &FakeMailer::default()).await;
    let mut db_context = persistence_test::db_context(&database).await;

    let status = send_webhook(
        &server,
        "/webhooks/sonarr/v1",
        SONARR_WEBHOOK_PASSWORD,
        HeaderMap::new(),
        &fixture("sonarr/webhooks/download.json"),
    )
    .await;

    eventually(async || {
        db_context
            .episodes()
            .filter(Episode::fields().in_library().eq(true))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    })
    .await;
    let show = db_context
        .shows()
        .filter(Show::fields().sonarr_id().eq(7))
        .one()
        .exec(db_context.executor_mut())
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(show.title, "Severance");
    drop(db_context);
    drop(server);
    database.drop().await;
}
