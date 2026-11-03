#[path = "../test/library.rs"]
mod library_test;
#[path = "../test/persistence.rs"]
mod persistence_test;

use application::ports::persistence::DbScope;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::Episode;
use domain::Movie;
use domain::Show;
use library_test::MediaFolder;
use library_test::StubRadarr;
use library_test::StubSonarr;
use library_test::eventually;
use library_test::fixture;
use library_test::selfless_test;
use library_test::selfless_test::RADARR_WEBHOOK_PASSWORD;
use library_test::selfless_test::SONARR_WEBHOOK_PASSWORD;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use serde_json::Value;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;
use test_fixtures::TestServer;

async fn send(server: &TestServer, path: &str, password: &str, body: &str) {
    let mut headers = HeaderMap::new();
    let credentials = STANDARD.encode(format!("selfless:{password}"));
    headers.insert(
        AUTHORIZATION,
        format!("Basic {credentials}").parse().unwrap(),
    );
    let (status, _) = server
        .request_with_headers(
            Method::POST,
            path,
            None,
            headers,
            Some(serde_json::from_str(body).unwrap()),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_radarr_download_delivery_re_syncs_its_movie_while_the_server_runs() {
    let database = TestDatabase::create().await;
    let radarr = StubRadarr::start().await;
    let media = MediaFolder::new();
    media.create("Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv");
    radarr
        .serve_movie(42, StatusCode::OK, &media.fixture("radarr/api/movie.json"))
        .await;
    let mut settings = selfless_test::settings(database.url());
    settings.radarr.url = Some(radarr.url());
    let server = selfless_test::start(settings, &FakeClock::new(), &FakeMailer::default()).await;
    let mut db_context = persistence_test::db_context(&database).await;

    send(
        &server,
        "/webhooks/radarr/v1",
        RADARR_WEBHOOK_PASSWORD,
        &fixture("radarr/webhooks/download.json"),
    )
    .await;

    let movie = eventually(async || {
        db_context
            .movies()
            .filter(Movie::fields().visible().eq(true))
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    })
    .await;
    assert_eq!(movie.title, "Arrival");
    drop(db_context);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_sonarr_download_delivery_re_syncs_its_show_while_the_server_runs() {
    let database = TestDatabase::create().await;
    let sonarr = StubSonarr::start().await;
    let media = MediaFolder::new();
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

    send(
        &server,
        "/webhooks/sonarr/v1",
        SONARR_WEBHOOK_PASSWORD,
        &fixture("sonarr/webhooks/download.json"),
    )
    .await;

    eventually(async || {
        db_context
            .episodes()
            .filter(Episode::fields().visible().eq(true))
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
    assert_eq!(show.title, "Severance");
    drop(db_context);
    drop(server);
    database.drop().await;
}
