mod support;

use application::sync_status::FailedRow;
use domain::Source;
use jiff::SignedDuration;
use jiff::Timestamp;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use support::MediaFolder;
use support::RADARR_WEBHOOK_PASSWORD;
use support::StubRadarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;

const RADARR_ID: i64 = 42;
const DUNE_TMDB_ID: i64 = 438631;
const FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";

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
        let media = MediaFolder::new();
        media.create(FILE);
        radarr.fail_movie(RADARR_ID).await;
        Self {
            database,
            server,
            radarr,
            media,
        }
    }

    async fn radarr_serves_the_movie(&self) {
        self.radarr
            .serve_movie(
                RADARR_ID,
                StatusCode::OK,
                &self.media.fixture("radarr/api/movie.json"),
            )
            .await;
    }

    async fn send(&self, webhook: &str) {
        self.send_body(&fixture(&format!("radarr/webhooks/{webhook}.json")))
            .await;
    }

    async fn send_body(&self, body: &str) {
        let status = self
            .server
            .send_radarr_webhook(
                Method::POST,
                Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
                body,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn radarr_was_asked(&self, times: usize) {
        eventually(async || (self.radarr.movie_requests(RADARR_ID).await == times).then_some(()))
            .await;
    }

    async fn process_after(&self, delay: SignedDuration) {
        self.server.clock.advance(delay);
        application::process_inbox_row::call(&self.server.state)
            .await
            .unwrap();
    }

    async fn delete_old_rows_after(&self, since: Timestamp, age: SignedDuration) -> u64 {
        let now = self.server.clock.now();
        self.server.clock.advance(age - since.duration_until(now));
        application::delete_old_inbox_rows::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn failed_row(&self) -> FailedRow {
        self.until_failed(1).await.pop().unwrap()
    }

    async fn until_failed(&self, count: usize) -> Vec<FailedRow> {
        eventually(async || {
            self.process_after(SignedDuration::from_hours(1)).await;
            let failed_rows = self.failed_rows().await;
            (failed_rows.len() == count).then_some(failed_rows)
        })
        .await
    }

    async fn reconcile_lists(&self, ids: &[i64]) {
        let movies: Vec<Value> =
            serde_json::from_str(&self.media.fixture("radarr/api/movies.json")).unwrap();
        let movies = movies
            .into_iter()
            .filter(|x| ids.contains(&x["id"].as_i64().unwrap()))
            .collect::<Vec<_>>();
        for movie in &movies {
            self.media
                .create_absolute(movie["movieFile"]["path"].as_str().unwrap());
        }
        self.radarr
            .serve_movies(StatusCode::OK, &Value::Array(movies).to_string())
            .await;
        self.server.clock.advance(SignedDuration::from_mins(1));
        let started_at = self.server.clock.now();
        application::sync_now::call(&self.server.state);
        eventually(async || {
            application::sync_status::call(&self.server.state)
                .await
                .unwrap()
                .radarr
                .last_reconcile
                .is_some_and(|x| x.started_at == started_at)
                .then_some(())
        })
        .await;
    }

    async fn failed_rows(&self) -> Vec<FailedRow> {
        application::sync_status::call(&self.server.state)
            .await
            .unwrap()
            .failed_rows
    }

    async fn movie_titles(&self) -> Vec<String> {
        application::list_movies::call(&self.server.state)
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.title)
            .collect()
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

#[tokio::test]
async fn a_delivery_that_fails_is_retried_after_a_minute_and_can_succeed() {
    let fixture = Fixture::start().await;
    fixture.send("download").await;
    fixture.radarr_was_asked(1).await;

    fixture.radarr_serves_the_movie().await;
    fixture.process_after(SignedDuration::from_mins(1)).await;

    eventually(async || (fixture.movie_titles().await == ["Arrival"]).then_some(())).await;
    assert!(fixture.failed_rows().await.is_empty());
    fixture.stop().await;
}

#[tokio::test]
async fn a_delivery_that_keeps_failing_is_retried_after_1_5_15_and_30_minutes_then_shows_as_failed()
{
    let fixture = Fixture::start().await;
    let received_at = fixture.server.clock.now();
    fixture.send("download").await;

    for (attempt, delay) in [(1, 1), (2, 5), (3, 15), (4, 30)] {
        fixture.radarr_was_asked(attempt).await;
        fixture
            .process_after(SignedDuration::from_mins(delay) - SignedDuration::from_secs(1))
            .await;
        assert_eq!(fixture.radarr.movie_requests(RADARR_ID).await, attempt);
        assert!(fixture.failed_rows().await.is_empty());
        fixture.process_after(SignedDuration::from_secs(1)).await;
    }
    fixture.radarr_was_asked(5).await;

    let failed = eventually(async || fixture.failed_rows().await.pop()).await;
    assert_eq!(failed.source, Source::Radarr);
    assert_eq!(failed.event_type.as_deref(), Some("Download"));
    assert_eq!(failed.attempts, 5);
    assert_eq!(failed.received_at, received_at);
    assert!(
        failed.error.contains("500"),
        "the error names Radarr's response: {}",
        failed.error
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_admin_retries_a_failed_delivery() {
    let fixture = Fixture::start().await;
    fixture.send("download").await;
    let failed = fixture.failed_row().await;

    fixture.radarr_serves_the_movie().await;
    application::retry_failed_row::call(&fixture.server.state, failed.id)
        .await
        .unwrap();

    eventually(async || (fixture.movie_titles().await == ["Arrival"]).then_some(())).await;
    assert!(fixture.failed_rows().await.is_empty());
    fixture.stop().await;
}

#[tokio::test]
async fn a_delivery_selfless_cannot_read_shows_as_failed() {
    let fixture = Fixture::start().await;
    let received_at = fixture.server.clock.now();

    fixture.send_body("not json").await;

    let failed = fixture.failed_row().await;
    assert_eq!(failed.source, Source::Radarr);
    assert_eq!(failed.event_type, None);
    assert_eq!(failed.attempts, 5);
    assert_eq!(failed.received_at, received_at);
    assert!(!failed.error.is_empty());
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_that_syncs_the_movie_resolves_its_failed_deliveries() {
    let fixture = Fixture::start().await;
    fixture.radarr.fail_movie(43).await;
    fixture.send("download").await;
    let mut other_movie: Value =
        serde_json::from_str(&support::fixture("radarr/webhooks/download.json")).unwrap();
    other_movie["movie"]["id"] = 43.into();
    other_movie["movie"]["tmdbId"] = DUNE_TMDB_ID.into();
    fixture.send_body(&other_movie.to_string()).await;
    fixture.until_failed(2).await;

    fixture.reconcile_lists(&[RADARR_ID]).await;

    assert_eq!(fixture.failed_rows().await.len(), 1);
    assert_eq!(fixture.movie_titles().await, ["Arrival"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_that_syncs_the_movie_under_a_new_radarr_id_resolves_its_failed_deliveries() {
    let fixture = Fixture::start().await;
    fixture.radarr.fail_movie(99).await;
    let mut download: Value =
        serde_json::from_str(&support::fixture("radarr/webhooks/download.json")).unwrap();
    download["movie"]["id"] = 99.into();
    fixture.send_body(&download.to_string()).await;
    fixture.until_failed(1).await;

    fixture.reconcile_lists(&[RADARR_ID]).await;

    assert!(fixture.failed_rows().await.is_empty());
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_that_removes_the_movie_resolves_its_failed_delete_deliveries() {
    let fixture = Fixture::start().await;
    fixture.reconcile_lists(&[RADARR_ID, 43]).await;
    fixture.radarr.fail_movie(43).await;
    let mut movie_delete: Value =
        serde_json::from_str(&support::fixture("radarr/webhooks/movie-delete.json")).unwrap();
    movie_delete["movie"]["id"] = 43.into();
    movie_delete["movie"]["tmdbId"] = DUNE_TMDB_ID.into();
    fixture.send_body(&movie_delete.to_string()).await;
    fixture.until_failed(1).await;

    fixture.reconcile_lists(&[RADARR_ID]).await;

    assert!(fixture.failed_rows().await.is_empty());
    assert_eq!(fixture.movie_titles().await, ["Arrival"]);
    fixture.stop().await;
}

#[tokio::test]
async fn processed_ignored_and_resolved_deliveries_are_deleted_30_days_after_they_were_received() {
    let fixture = Fixture::start().await;
    let first_received_at = fixture.server.clock.now();
    fixture.send("test").await;
    fixture.send("download").await;
    fixture.send_body("not json").await;
    fixture.until_failed(2).await;
    fixture.reconcile_lists(&[RADARR_ID]).await;
    fixture
        .radarr
        .serve_movie(
            RADARR_ID,
            StatusCode::OK,
            &fixture.media.fixture("radarr/api/movie-without-file.json"),
        )
        .await;
    let last_received_at = fixture.server.clock.now();
    fixture.send("movie-file-delete").await;
    eventually(async || fixture.movie_titles().await.is_empty().then_some(())).await;

    let thirty_days = SignedDuration::from_hours(30 * 24);
    let one_second = SignedDuration::from_secs(1);
    assert_eq!(
        fixture
            .delete_old_rows_after(first_received_at, thirty_days - one_second)
            .await,
        0
    );
    assert_eq!(
        fixture
            .delete_old_rows_after(first_received_at, thirty_days)
            .await,
        2
    );
    assert_eq!(
        fixture
            .delete_old_rows_after(last_received_at, thirty_days)
            .await,
        1
    );
    assert_eq!(fixture.failed_rows().await.len(), 1);
    fixture.stop().await;
}
