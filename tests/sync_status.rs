#[path = "../test/library.rs"]
mod library_test;

use application::library::sync_status::Output as SyncStatus;
use application::library::sync_status::Problem;
use application::library::sync_status::ProblemItem;
use application::library::sync_status::SourceStatus;
use domain::SyncProblemKind;
use jiff::SignedDuration;
use library_test::LibraryServer;
use library_test::MediaFolder;
use library_test::StubRadarr;
use library_test::StubSonarr;
use library_test::eventually;
use library_test::selfless_test::settings;
use reqwest::StatusCode;
use std::time::Duration;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;

const SEVERANCE: i64 = 7;
const SEVERANCE_FILES: [&str; 3] = [
    "TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p.mkv",
    "TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv",
    "TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv",
];
const SEVERANCE_TWO_EPISODE_FILE: &str =
    "TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p.mkv";

struct Fixture {
    database: TestDatabase,
    server: LibraryServer,
    sonarr: StubSonarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start() -> Self {
        let database = TestDatabase::create().await;
        let radarr = StubRadarr::start().await;
        let sonarr = StubSonarr::start().await;
        radarr.serve_movies(StatusCode::OK, "[]").await;
        sonarr.serve_series_list("[]").await;
        let mut settings = settings(database.url());
        settings.radarr.url = Some(radarr.url());
        settings.sonarr.url = Some(sonarr.url());
        let server = LibraryServer::start_with(settings, FakeClock::new()).await;
        let fixture = Self {
            database,
            server,
            sonarr,
            media: MediaFolder::new(),
        };
        fixture.reconciled().await;
        fixture
    }

    async fn status(&self) -> SyncStatus {
        application::library::sync_status::call(&self.server.state)
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
            (reconciled(&status.radarr) && reconciled(&status.sonarr)).then_some(())
        })
        .await;
        self.status().await
    }

    async fn sync_now(&self) -> SyncStatus {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::library::sync_now::call(&self.server.state).unwrap();
        self.reconciled().await
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
async fn an_episode_file_selfless_cannot_find_is_listed_for_each_of_its_episodes() {
    let fixture = Fixture::start().await;
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
