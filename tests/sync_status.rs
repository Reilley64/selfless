mod support;

use application::ports::Probe;
use application::sync_status::ApiCall;
use application::sync_status::Problem;
use application::sync_status::ProblemItem;
use application::sync_status::SourceStatus;
use application::sync_status::SyncStatus;
use domain::SyncProblemKind;
use jiff::SignedDuration;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use support::FakeClock;
use support::MediaFolder;
use support::RADARR_WEBHOOK_PASSWORD;
use support::StubRadarr;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;
use support::settings;

const ARRIVAL: i64 = 42;
const SEVERANCE: i64 = 7;
const ARRIVAL_FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";
const SEVERANCE_FILES: [&str; 3] = [
    "TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p.mkv",
    "TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv",
    "TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv",
];
const SEVERANCE_TWO_EPISODE_FILE: &str =
    "TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p.mkv";

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    radarr: StubRadarr,
    sonarr: StubSonarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start(radarr_kodi: bool, sonarr_kodi: bool) -> Self {
        let database = TestDatabase::create().await;
        let radarr = StubRadarr::start().await;
        let sonarr = StubSonarr::start().await;
        serve(&radarr, &sonarr, radarr_kodi, sonarr_kodi).await;
        let mut settings = settings(database.url());
        settings.radarr.url = Some(radarr.url());
        settings.sonarr.url = Some(sonarr.url());
        let server = TestServer::start_with(settings, FakeClock::new()).await;
        let fixture = Self {
            database,
            server,
            radarr,
            sonarr,
            media: MediaFolder::new(),
        };
        fixture.reconciled().await;
        fixture
    }

    async fn status(&self) -> SyncStatus {
        application::sync_status::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn reconciled(&self) -> SyncStatus {
        let started_at = self.server.clock.now();
        let reconciled = |x: &SourceStatus| {
            x.last_reconcile
                .as_ref()
                .is_some_and(|x| x.started_at == started_at)
        };
        eventually(async || {
            let status = self.status().await;
            (reconciled(&status.radarr) && reconciled(&status.sonarr)).then_some(status)
        })
        .await
    }

    async fn sync_now(&self) -> SyncStatus {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::sync_now::call(&self.server.state);
        self.reconciled().await
    }

    async fn radarr_lists_arrival(&self) {
        let movies: Vec<Value> =
            serde_json::from_str(&self.media.fixture("radarr/api/movies.json")).unwrap();
        let arrival = movies.into_iter().find(|x| x["id"] == ARRIVAL).unwrap();
        self.radarr
            .serve_movies(StatusCode::OK, &format!("[{arrival}]"))
            .await;
    }

    async fn sonarr_lists_severance(&self) {
        self.sonarr
            .serve_series_list(&format!(
                "[{}]",
                self.media.fixture("sonarr/api/series.json")
            ))
            .await;
        self.sonarr
            .serve_episodes(
                SEVERANCE,
                &self.media.fixture("sonarr/api/episodes.json"),
                &self.media.fixture("sonarr/api/episode-files.json"),
                Duration::ZERO,
            )
            .await;
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

#[tokio::test]
async fn a_movie_file_ffprobe_cannot_read_is_listed_as_ffprobe_failed_until_it_is_read() {
    let fixture = Fixture::start(true, true).await;
    fixture.media.create(ARRIVAL_FILE);
    let path = fixture.media.path(ARRIVAL_FILE);
    fixture.server.prober.fail(&path);
    fixture.radarr_lists_arrival().await;

    let status = fixture.sync_now().await;

    let [problem] = status.problems.as_slice() else {
        panic!("one problem, not {:?}", status.problems);
    };
    assert_eq!(problem.kind, SyncProblemKind::FfprobeFailed);
    assert_eq!(problem.path, path);
    assert!(
        matches!(&problem.item, ProblemItem::Movie { title, .. } if title == "Arrival"),
        "the problem names Arrival: {:?}",
        problem.item
    );

    fixture.media.change_absolute(&path);
    fixture.server.prober.serve(
        &path,
        Probe {
            container: "matroska,webm".to_owned(),
            duration: None,
            video_streams: Vec::new(),
            audio_streams: Vec::new(),
            subtitle_streams: Vec::new(),
        },
    );
    assert_eq!(fixture.sync_now().await.problems, []);
    fixture.stop().await;
}

async fn serve(radarr: &StubRadarr, sonarr: &StubSonarr, radarr_kodi: bool, sonarr_kodi: bool) {
    radarr.serve_movies(StatusCode::OK, "[]").await;
    radarr
        .serve_metadata(&metadata("radarr/api/metadata.json", radarr_kodi))
        .await;
    sonarr.serve_series_list("[]").await;
    sonarr
        .serve_metadata(&metadata("sonarr/api/metadata.json", sonarr_kodi))
        .await;
}

fn metadata(path: &str, kodi: bool) -> String {
    let mut consumers: Vec<Value> = serde_json::from_str(&fixture(path)).unwrap();
    for consumer in &mut consumers {
        if consumer["implementation"] == "XbmcMetadata" {
            consumer["enable"] = kodi.into();
        }
    }
    Value::Array(consumers).to_string()
}

#[tokio::test]
async fn each_reconcile_reads_whether_kodi_metadata_is_on_in_radarr_and_sonarr() {
    let fixture = Fixture::start(true, false).await;
    let status = fixture.status().await;
    assert_eq!(status.radarr.kodi_metadata, Some(true));
    assert_eq!(status.sonarr.kodi_metadata, Some(false));

    serve(&fixture.radarr, &fixture.sonarr, false, true).await;
    let status = fixture.sync_now().await;

    assert_eq!(status.radarr.kodi_metadata, Some(false));
    assert_eq!(status.sonarr.kodi_metadata, Some(true));
    fixture.stop().await;
}

#[tokio::test]
async fn the_sync_status_shows_whether_the_last_call_to_radarr_and_sonarr_succeeded() {
    let fixture = Fixture::start(true, true).await;
    assert_eq!(
        fixture.status().await.radarr.last_api_call,
        Some(ApiCall::Succeeded)
    );

    fixture
        .radarr
        .serve_movies(StatusCode::UNAUTHORIZED, "{}")
        .await;
    fixture
        .sonarr
        .serve_series_list(&format!(
            "[{}]",
            fixture.media.fixture("sonarr/api/series.json")
        ))
        .await;
    fixture.sonarr.fail_episodes(SEVERANCE).await;
    let failed = fixture.sync_now().await;

    assert_failed(&failed.radarr, "401");
    assert_failed(&failed.sonarr, "500");

    serve(&fixture.radarr, &fixture.sonarr, true, true).await;
    let succeeded = fixture.sync_now().await;

    assert_eq!(succeeded.radarr.last_api_call, Some(ApiCall::Succeeded));
    assert_eq!(succeeded.sonarr.last_api_call, Some(ApiCall::Succeeded));
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_call_from_a_webhook_re_sync_shows_in_the_sync_status() {
    let fixture = Fixture::start(true, true).await;

    fixture.radarr.fail_movie(ARRIVAL).await;
    let status = fixture
        .server
        .send_radarr_webhook(
            Method::POST,
            Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
            &support::fixture("radarr/webhooks/download.json"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let radarr = eventually(async || {
        let radarr = fixture.status().await.radarr;
        (radarr.last_api_call != Some(ApiCall::Succeeded)).then_some(radarr)
    })
    .await;
    assert_failed(&radarr, "500");
    fixture.stop().await;
}

fn assert_failed(status: &SourceStatus, http_status: &str) {
    match &status.last_api_call {
        Some(ApiCall::Failed(error)) => assert!(
            error.contains(http_status),
            "the error names the response status {http_status}: {error}"
        ),
        other => panic!("the last call failed, not {other:?}"),
    }
}

#[tokio::test]
async fn a_movie_file_selfless_cannot_find_is_listed_as_file_not_found_until_it_appears() {
    let fixture = Fixture::start(true, true).await;
    fixture.radarr_lists_arrival().await;

    let status = fixture.sync_now().await;

    let [problem] = status.problems.as_slice() else {
        panic!("one problem, not {:?}", status.problems);
    };
    assert_eq!(problem.kind, SyncProblemKind::FileNotFound);
    assert_eq!(problem.path, fixture.media.path(ARRIVAL_FILE));
    assert!(
        matches!(&problem.item, ProblemItem::Movie { title, .. } if title == "Arrival"),
        "the problem names Arrival: {:?}",
        problem.item
    );

    fixture.media.create(ARRIVAL_FILE);
    fixture.radarr_lists_arrival().await;
    assert_eq!(fixture.sync_now().await.problems, []);
    fixture.stop().await;
}

#[tokio::test]
async fn an_episode_file_selfless_cannot_find_is_listed_for_each_of_its_episodes() {
    let fixture = Fixture::start(true, true).await;
    for file in SEVERANCE_FILES {
        fixture.media.create(file);
    }
    fixture.sonarr_lists_severance().await;

    let status = fixture.sync_now().await;

    let path = fixture.media.path(SEVERANCE_TWO_EPISODE_FILE);
    let items = status
        .problems
        .iter()
        .map(|x| {
            assert_eq!(x.kind, SyncProblemKind::FileNotFound);
            assert_eq!(x.path, path);
            match &x.item {
                ProblemItem::Episode {
                    show,
                    season,
                    number,
                    title,
                    ..
                } => (show.as_str(), *season, *number, title.as_str()),
                other => panic!("the problem names an episode, not {other:?}"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        items,
        [
            ("Severance", 1, 2, "Half Loop"),
            ("Severance", 1, 3, "In Perpetuity")
        ]
    );

    fixture.media.create(SEVERANCE_TWO_EPISODE_FILE);
    fixture.sonarr_lists_severance().await;
    assert_eq!(fixture.sync_now().await.problems, Vec::<Problem>::new());
    fixture.stop().await;
}
