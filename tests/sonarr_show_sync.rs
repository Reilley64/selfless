mod support;

use application::list_episodes::Episode;
use application::list_shows::Show;
use jiff::Timestamp;
use jiff::civil::date;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use support::MediaFolder;
use support::SONARR_WEBHOOK_PASSWORD;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;
use uuid::Uuid;

const SONARR_ID: i64 = 7;
const S01E01: &str =
    "TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv";
const S02E01: &str = "TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv";

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    sonarr: StubSonarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start() -> Self {
        let database = TestDatabase::create().await;
        let sonarr = StubSonarr::start().await;
        let server = TestServer::start_with_sonarr(database.url(), &sonarr).await;
        Self {
            database,
            server,
            sonarr,
            media: MediaFolder::new(),
        }
    }

    async fn sonarr_serves(&self, episodes: &str, episode_files: &str) {
        self.sonarr
            .serve_series(
                SONARR_ID,
                &self.media.fixture("sonarr/api/series.json"),
                &self.media.fixture(&format!("sonarr/api/{episodes}.json")),
                &self
                    .media
                    .fixture(&format!("sonarr/api/{episode_files}.json")),
            )
            .await;
    }

    fn create_files(&self, episode_files: &str) {
        let files: Vec<Value> =
            serde_json::from_str(&fixture(&format!("sonarr/api/{episode_files}.json"))).unwrap();
        for file in files {
            self.media.create(
                file["path"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("/mnt/media/")
                    .unwrap(),
            );
        }
    }

    async fn send(&self, webhook: &str) {
        let status = self
            .server
            .send_sonarr_webhook(
                Method::POST,
                Some(("sonarr", SONARR_WEBHOOK_PASSWORD)),
                &fixture(&format!("sonarr/webhooks/{webhook}.json")),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    async fn shows(&self) -> Vec<Show> {
        application::list_shows::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn visible_show(&self) -> Show {
        eventually(async || self.shows().await.pop()).await
    }

    async fn hidden(&self) {
        eventually(async || self.shows().await.is_empty().then_some(())).await;
    }

    async fn season_numbers(&self, show_id: Uuid) -> Vec<i64> {
        application::list_seasons::call(&self.server.state, show_id)
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.number)
            .collect()
    }

    async fn episodes(&self, show_id: Uuid, season_number: i64) -> Vec<Episode> {
        let seasons = application::list_seasons::call(&self.server.state, show_id)
            .await
            .unwrap();
        let Some(season) = seasons.into_iter().find(|x| x.number == season_number) else {
            return Vec::new();
        };
        application::list_episodes::call(&self.server.state, season.id)
            .await
            .unwrap()
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

fn numbers(episodes: &[Episode]) -> Vec<i64> {
    episodes.iter().map(|x| x.number).collect()
}

#[tokio::test]
async fn a_download_for_one_episode_file_adds_a_visible_show_with_sonarr_metadata() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;

    fixture.send("download").await;

    let show = fixture.visible_show().await;
    assert_eq!(show.title, "Severance");
    assert_eq!(show.year, Some(2022));
    assert_eq!(
        show.genres,
        ["Drama", "Mystery", "Science Fiction", "Thriller"]
    );
    assert!(
        show.overview
            .as_deref()
            .unwrap()
            .starts_with("Mark leads a team of office workers")
    );
    let episodes = fixture.episodes(show.id, 2).await;
    assert_eq!(numbers(&episodes), [1]);
    assert_eq!(episodes[0].title, "Hello, Ms. Cobel");
    assert_eq!(episodes[0].air_date, Some(date(2025, 1, 17)));
    assert_eq!(
        episodes[0].overview.as_deref(),
        Some("Mark leads his team on a search for answers.")
    );
    assert_eq!(
        episodes[0].added_at,
        Some("2025-01-17T03:12:44Z".parse::<Timestamp>().unwrap())
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_import_complete_download_re_syncs_the_show() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;

    fixture.send("download-import-complete").await;

    assert_eq!(fixture.visible_show().await.title, "Severance");
    fixture.stop().await;
}

#[tokio::test]
async fn rename_series_add_and_episode_file_delete_re_sync_the_show() {
    for webhook in ["rename", "series-add", "episode-file-delete"] {
        let fixture = Fixture::start().await;
        fixture.create_files("episode-files");
        fixture.sonarr_serves("episodes", "episode-files").await;

        fixture.send(webhook).await;

        assert_eq!(fixture.visible_show().await.title, "Severance", "{webhook}");
        fixture.stop().await;
    }
}

#[tokio::test]
async fn specials_are_listed_after_the_numbered_seasons() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;

    fixture.send("download").await;

    let show = fixture.visible_show().await;
    assert_eq!(fixture.season_numbers(show.id).await, [1, 2, 0]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_file_holding_two_episodes_gives_each_episode_its_own_row_pointing_at_the_one_file() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;

    fixture.send("download-import-complete").await;

    let show = fixture.visible_show().await;
    let episodes = fixture.episodes(show.id, 1).await;
    assert_eq!(
        episodes
            .iter()
            .map(|x| (x.number, x.title.as_str()))
            .collect::<Vec<_>>(),
        [
            (1, "Good News About Hell"),
            (2, "Half Loop"),
            (3, "In Perpetuity")
        ]
    );
    assert_ne!(episodes[1].id, episodes[2].id);
    assert!(episodes[1].media_file_id.is_some());
    assert_eq!(episodes[1].media_file_id, episodes[2].media_file_id);
    assert_ne!(episodes[0].media_file_id, episodes[1].media_file_id);
    fixture.stop().await;
}

#[tokio::test]
async fn only_episodes_with_a_file_on_disk_are_visible_and_seasons_and_shows_follow_them() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.media.remove(S01E01);
    fixture.sonarr_serves("episodes", "episode-files").await;

    fixture.send("download").await;

    let show = fixture.visible_show().await;
    assert_eq!(numbers(&fixture.episodes(show.id, 1).await), [2, 3]);
    assert_eq!(numbers(&fixture.episodes(show.id, 2).await), [1]);
    assert_eq!(fixture.season_numbers(show.id).await, [1, 2, 0]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_show_without_files_is_hidden_and_comes_back_with_the_same_rows() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;
    fixture.send("download").await;
    let before = fixture.visible_show().await;
    let episodes_before = fixture.episodes(before.id, 1).await;

    fixture
        .sonarr_serves("episodes", "episode-files-none")
        .await;
    fixture.send("episode-file-delete").await;
    fixture.hidden().await;
    fixture.sonarr_serves("episodes", "episode-files").await;
    fixture.send("download").await;

    let after = fixture.visible_show().await;
    assert_eq!(after.id, before.id);
    let episodes_after = fixture.episodes(after.id, 1).await;
    assert_eq!(
        episodes_after.iter().map(|x| x.id).collect::<Vec<_>>(),
        episodes_before.iter().map(|x| x.id).collect::<Vec<_>>()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_upgrade_keeps_the_episode_and_its_added_date() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;
    fixture.send("download").await;
    let show = fixture.visible_show().await;
    let before = fixture.episodes(show.id, 2).await.pop().unwrap();

    fixture.media.remove(S02E01);
    fixture.create_files("episode-files-after-upgrade");
    fixture
        .sonarr_serves("episodes-after-upgrade", "episode-files-after-upgrade")
        .await;
    fixture.send("download-upgrade").await;

    let after = eventually(async || {
        fixture
            .episodes(show.id, 2)
            .await
            .pop()
            .filter(|x| x.media_file_id != before.media_file_id)
    })
    .await;
    assert_eq!(after.id, before.id);
    assert_eq!(
        after.added_at,
        Some("2025-01-17T03:12:44Z".parse::<Timestamp>().unwrap())
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_series_delete_removes_the_show_with_its_seasons_and_episodes() {
    let fixture = Fixture::start().await;
    fixture.create_files("episode-files");
    fixture.sonarr_serves("episodes", "episode-files").await;
    fixture.send("download").await;
    let before = fixture.visible_show().await;
    let episodes_before = fixture.episodes(before.id, 1).await;

    fixture.sonarr.forget_series(SONARR_ID).await;
    fixture.send("series-delete").await;
    fixture.hidden().await;
    fixture.sonarr_serves("episodes", "episode-files").await;
    fixture.send("series-add").await;

    let after = fixture.visible_show().await;
    assert_ne!(after.id, before.id);
    let episodes_after = fixture.episodes(after.id, 1).await;
    assert_eq!(numbers(&episodes_after), [1, 2, 3]);
    assert!(
        episodes_after
            .iter()
            .all(|x| episodes_before.iter().all(|y| y.id != x.id))
    );
    fixture.stop().await;
}
