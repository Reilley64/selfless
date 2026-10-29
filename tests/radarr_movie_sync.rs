mod support;

use application::list_movies::Movie;
use jiff::Timestamp;
use reqwest::Method;
use reqwest::StatusCode;
use support::MediaFolder;
use support::RADARR_WEBHOOK_PASSWORD;
use support::StubRadarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;

const RADARR_ID: i64 = 42;
const FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";
const UPGRADED_FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Remux-2160p.mkv";

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    radarr: StubRadarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start() -> Self {
        let database = TestDatabase::create().await;
        let radarr = StubRadarr::start().await;
        let server = TestServer::start_with_radarr(database.url(), &radarr).await;
        Self {
            database,
            server,
            radarr,
            media: MediaFolder::new(),
        }
    }

    async fn radarr_serves(&self, name: &str) {
        self.radarr
            .serve_movie(
                RADARR_ID,
                StatusCode::OK,
                &self.media.fixture(&format!("radarr/api/{name}.json")),
            )
            .await;
    }

    async fn radarr_forgets(&self) {
        self.radarr
            .serve_movie(
                RADARR_ID,
                StatusCode::NOT_FOUND,
                &fixture("radarr/api/movie-not-found.json"),
            )
            .await;
    }

    async fn send(&self, webhook: &str) {
        let status = self
            .server
            .send_radarr_webhook(
                Method::POST,
                Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
                &fixture(&format!("radarr/webhooks/{webhook}.json")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn movies(&self) -> Vec<Movie> {
        application::list_movies::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn visible_movie(&self) -> Movie {
        eventually(async || self.movies().await.pop()).await
    }

    async fn hidden(&self) {
        eventually(async || self.movies().await.is_empty().then_some(())).await;
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

#[tokio::test]
async fn a_download_adds_a_visible_movie_with_radarr_metadata() {
    let fixture = Fixture::start().await;
    fixture.media.create(FILE);
    fixture.radarr_serves("movie").await;

    fixture.send("download").await;

    let movie = fixture.visible_movie().await;
    assert_eq!(movie.title, "Arrival");
    assert_eq!(movie.year, 2016);
    assert_eq!(movie.genres, ["Drama", "Science Fiction", "Mystery"]);
    assert_eq!(
        movie.overview.as_deref(),
        Some(
            "Taking place after alien crafts land around the world, an expert linguist is \
             recruited by the military to determine whether they come in peace or are a threat."
        )
    );
    assert_eq!(
        movie.added_at,
        Some("2026-10-29T10:00:00Z".parse::<Timestamp>().unwrap())
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_movie_without_a_file_is_hidden_and_comes_back_when_a_file_returns() {
    let fixture = Fixture::start().await;
    fixture.media.create(FILE);
    fixture.radarr_serves("movie").await;
    fixture.send("download").await;
    let before = fixture.visible_movie().await;

    fixture.radarr_serves("movie-without-file").await;
    fixture.send("movie-file-delete").await;
    fixture.hidden().await;
    fixture.radarr_serves("movie").await;
    fixture.send("download").await;

    assert_eq!(fixture.visible_movie().await.id, before.id);
    fixture.stop().await;
}

#[tokio::test]
async fn a_movie_whose_file_path_is_missing_on_disk_is_hidden() {
    let fixture = Fixture::start().await;
    fixture.media.create(FILE);
    fixture.radarr_serves("movie").await;
    fixture.send("download").await;
    let before = fixture.visible_movie().await;

    fixture.media.remove(FILE);
    fixture.send("rename").await;
    fixture.hidden().await;
    fixture.media.create(FILE);
    fixture.send("download").await;

    assert_eq!(fixture.visible_movie().await.id, before.id);
    fixture.stop().await;
}

#[tokio::test]
async fn an_upgrade_keeps_the_movie_and_its_added_date() {
    let fixture = Fixture::start().await;
    fixture.media.create(FILE);
    fixture.radarr_serves("movie").await;
    fixture.send("download").await;
    let before = fixture.visible_movie().await;

    fixture.radarr_serves("movie-without-file").await;
    fixture.send("movie-file-delete").await;
    fixture.hidden().await;
    fixture.media.remove(FILE);
    fixture.media.create(UPGRADED_FILE);
    fixture.radarr_serves("movie-upgraded").await;
    fixture.send("download-upgrade").await;

    let after = fixture.visible_movie().await;
    assert_eq!(after.id, before.id);
    assert_eq!(
        after.added_at,
        Some("2026-10-29T10:00:00Z".parse::<Timestamp>().unwrap())
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_movie_delete_removes_the_movie() {
    let fixture = Fixture::start().await;
    fixture.media.create(FILE);
    fixture.radarr_serves("movie").await;
    fixture.send("download").await;
    let before = fixture.visible_movie().await;

    fixture.radarr_forgets().await;
    fixture.send("movie-delete").await;
    fixture.hidden().await;
    fixture.radarr_serves("movie").await;
    fixture.send("movie-added").await;

    assert_ne!(fixture.visible_movie().await.id, before.id);
    fixture.stop().await;
}
