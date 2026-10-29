mod support;

use jiff::SignedDuration;
use opentelemetry::logs::AnyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::logs::InMemoryLogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::Browser;
use support::FakeClock;
use support::MediaFolder;
use support::Page;
use support::RADARR_WEBHOOK_PASSWORD;
use support::StubRadarr;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;
use support::settings;
use tracing::level_filters::LevelFilter;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::SubscriberExt;

const ARRIVAL: i64 = 42;
const DUNE: i64 = 43;
const ALL_MOVIES: [i64; 4] = [42, 43, 44, 45];
const SEVERANCE: i64 = 7;
const ARRIVAL_FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";
const SEVERANCE_FILES: [&str; 4] = [
    "TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p.mkv",
    "TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv",
    "TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p.mkv",
    "TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv",
];
const SOURCES_PATH: &str = "/admin/sync/sources";
const POLLING: &str = r#"hx-trigger="every 5s""#;

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    radarr: StubRadarr,
    sonarr: StubSonarr,
    media: MediaFolder,
    browser: Browser,
}

impl Fixture {
    async fn start() -> Self {
        Self::start_with_kodi_metadata(true, true).await
    }

    async fn start_with_kodi_metadata(radarr_kodi: bool, sonarr_kodi: bool) -> Self {
        let database = TestDatabase::create().await;
        let radarr = StubRadarr::start().await;
        let sonarr = StubSonarr::start().await;
        radarr.serve_movies(StatusCode::OK, "[]").await;
        radarr
            .serve_metadata(&kodi_metadata("radarr/api/metadata.json", radarr_kodi))
            .await;
        sonarr.serve_series_list("[]").await;
        sonarr
            .serve_metadata(&kodi_metadata("sonarr/api/metadata.json", sonarr_kodi))
            .await;
        let mut settings = settings(database.url());
        settings.radarr.url = Some(radarr.url());
        settings.sonarr.url = Some(sonarr.url());
        let server = TestServer::start_with(settings, FakeClock::new()).await;
        let mut browser = server.browser();
        browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
        let fixture = Self {
            database,
            server,
            radarr,
            sonarr,
            media: MediaFolder::new(),
            browser,
        };
        fixture.reconciled().await;
        fixture
    }

    async fn reconciled(&self) {
        let started_at = self.server.clock.now();
        eventually(async || {
            let status = application::sync_status::call(&self.server.state)
                .await
                .unwrap();
            [status.radarr, status.sonarr]
                .iter()
                .all(|x| {
                    x.last_reconcile
                        .as_ref()
                        .is_some_and(|x| x.started_at == started_at)
                })
                .then_some(())
        })
        .await;
    }

    async fn sync_now(&self) {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::sync_now::call(&self.server.state);
        self.reconciled().await;
    }

    fn listed_movies(&self, ids: &[i64]) -> String {
        let movies: Vec<Value> =
            serde_json::from_str(&self.media.fixture("radarr/api/movies.json")).unwrap();
        let movies = movies
            .into_iter()
            .filter(|x| ids.contains(&x["id"].as_i64().unwrap()))
            .collect();
        Value::Array(movies).to_string()
    }

    async fn radarr_lists(&self, ids: &[i64]) {
        let movies = self.listed_movies(ids);
        for movie in serde_json::from_str::<Vec<Value>>(&movies).unwrap() {
            self.media
                .create_absolute(movie["movieFile"]["path"].as_str().unwrap());
        }
        self.radarr.serve_movies(StatusCode::OK, &movies).await;
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

    async fn sync_page(&mut self) -> Page {
        let page = self.browser.get("/admin/sync").await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        page
    }

    async fn click(&mut self, page: &Page, button: &str) -> Page {
        let path = page.hx_post(button);
        let response = self
            .browser
            .htmx_post(&path, Some(&page.htmx_csrf_token()))
            .await;
        assert_eq!(response.status, StatusCode::OK, "{}", response.body);
        response
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

fn kodi_metadata(path: &str, enabled: bool) -> String {
    let mut consumers: Vec<Value> = serde_json::from_str(&fixture(path)).unwrap();
    for consumer in &mut consumers {
        if consumer["implementation"] == "XbmcMetadata" {
            consumer["enable"] = enabled.into();
        }
    }
    Value::Array(consumers).to_string()
}

fn capture_logs() -> (InMemoryLogExporter, DefaultGuard) {
    let exporter = InMemoryLogExporter::default();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let guard = tracing::subscriber::set_default(
        tracing_subscriber::registry()
            .with(LevelFilter::INFO)
            .with(OpenTelemetryTracingBridge::new(&provider)),
    );
    (exporter, guard)
}

fn admin_actions(exporter: &InMemoryLogExporter) -> Vec<BTreeMap<String, String>> {
    exporter
        .get_emitted_logs()
        .unwrap()
        .iter()
        .filter(|x| x.record.body() == Some(&AnyValue::from("admin action")))
        .map(|x| {
            x.record
                .attributes_iter()
                .map(|(key, value)| {
                    let AnyValue::String(value) = value else {
                        panic!("{key} is a string, not {value:?}");
                    };
                    (key.to_string(), value.to_string())
                })
                .collect()
        })
        .collect()
}

fn attributes<const N: usize>(pairs: [(&str, &str); N]) -> BTreeMap<String, String> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn assert_in_order(text: &str, expected: &[&str]) {
    let mut rest = text;
    for expected in expected {
        let at = rest
            .find(expected)
            .unwrap_or_else(|| panic!("{expected:?} in order in {text}"));
        rest = &rest[at + expected.len()..];
    }
}

#[tokio::test]
async fn each_source_card_shows_api_health_webhooks_the_last_reconcile_and_counts() {
    let mut fixture = Fixture::start().await;
    fixture.radarr_lists(&[ARRIVAL, DUNE]).await;
    for file in SEVERANCE_FILES {
        fixture.media.create(file);
    }
    fixture.sonarr_lists_severance().await;
    fixture.sync_now().await;
    let started_at = fixture
        .server
        .clock
        .now()
        .strftime("%Y-%m-%d %H:%M UTC")
        .to_string();

    let page = fixture.sync_page().await;

    let radarr = page.section_text("Radarr");
    let sonarr = page.section_text("Sonarr");
    for card in [&radarr, &sonarr] {
        for expected in [
            "API working",
            "No webhook received",
            "No rejected webhooks",
            "Last reconcile",
            "Succeeded",
            started_at.as_str(),
            "Took ",
        ] {
            assert!(card.contains(expected), "{expected:?} in {card}");
        }
        assert!(!card.contains("Kodi metadata is off"), "{card}");
        assert!(!card.contains("Removal held"), "{card}");
    }
    assert!(radarr.contains("2 movies"), "{radarr}");
    assert!(sonarr.contains("1 show"), "{sonarr}");
    assert!(sonarr.contains("5 episodes"), "{sonarr}");
    assert!(page.text().contains("Sync now"), "{}", page.body);
    assert!(
        page.section_text("Failed deliveries")
            .contains("No failed deliveries")
    );
    assert!(
        page.section_text("Items with problems")
            .contains("No items with problems")
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_call_shows_in_its_source_card_with_the_error() {
    let mut fixture = Fixture::start().await;
    fixture
        .radarr
        .serve_movies(StatusCode::UNAUTHORIZED, "{}")
        .await;
    fixture.sync_now().await;

    let page = fixture.sync_page().await;

    let radarr = page.section_text("Radarr");
    assert!(radarr.contains("API failing"), "{radarr}");
    assert!(radarr.contains("401"), "{radarr}");
    assert!(radarr.contains("Failed"), "{radarr}");
    assert!(page.section_text("Sonarr").contains("API working"));
    fixture.stop().await;
}

#[tokio::test]
async fn a_source_with_kodi_metadata_off_shows_the_kodi_warning() {
    let mut fixture = Fixture::start_with_kodi_metadata(true, false).await;

    let page = fixture.sync_page().await;

    let radarr = page.section_text("Radarr");
    let sonarr = page.section_text("Sonarr");
    assert!(!radarr.contains("Kodi metadata is off"), "{radarr}");
    assert!(sonarr.contains("Kodi metadata is off"), "{sonarr}");
    fixture.stop().await;
}

#[tokio::test]
async fn a_webhook_replaces_the_no_webhook_warning_and_rejected_webhooks_are_counted() {
    let mut fixture = Fixture::start().await;
    let received_at = fixture
        .server
        .clock
        .now()
        .strftime("%Y-%m-%d %H:%M UTC")
        .to_string();
    let webhook = support::fixture("radarr/webhooks/test.json");
    for credentials in [
        Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
        Some(("radarr", "wrong")),
        None,
    ] {
        fixture
            .server
            .send_radarr_webhook(Method::POST, credentials, &webhook)
            .await;
    }

    let page = fixture.sync_page().await;

    let radarr = page.section_text("Radarr");
    let sonarr = page.section_text("Sonarr");
    assert!(!radarr.contains("No webhook received"), "{radarr}");
    assert!(radarr.contains("Last webhook"), "{radarr}");
    assert!(radarr.contains(&received_at), "{radarr}");
    assert!(radarr.contains("2 rejected webhooks"), "{radarr}");
    assert!(sonarr.contains("No webhook received"), "{sonarr}");
    assert!(sonarr.contains("No rejected webhooks"), "{sonarr}");
    fixture.stop().await;
}

#[tokio::test]
async fn a_held_removal_shows_in_its_card_until_an_admin_confirms_it() {
    let mut fixture = Fixture::start().await;
    fixture.radarr_lists(&ALL_MOVIES).await;
    fixture.sync_now().await;
    fixture.radarr_lists(&[ARRIVAL]).await;
    fixture.sync_now().await;
    let (logs, _guard) = capture_logs();

    let page = fixture.sync_page().await;
    let radarr = page.section_text("Radarr");
    assert!(radarr.contains("Removal held"), "{radarr}");
    assert!(
        radarr.contains("Radarr no longer lists 3 movies"),
        "{radarr}"
    );
    assert!(radarr.contains("Confirm removal"), "{radarr}");
    assert!(radarr.contains("Remove 3 movies?"), "{radarr}");
    assert!(radarr.contains("4 movies"), "{radarr}");
    assert!(!page.section_text("Sonarr").contains("Removal held"));

    let confirmed = fixture.click(&page, "Remove 3 movies").await;

    assert!(
        confirmed.text().contains("Removal confirmed"),
        "{}",
        confirmed.body
    );
    let radarr = confirmed.section_text("Radarr");
    assert!(!radarr.contains("Removal held"), "{radarr}");
    assert!(radarr.contains("1 movie"), "{radarr}");
    assert_eq!(
        admin_actions(&logs),
        [attributes([
            ("selfless.admin.action", "sync.confirm_removal"),
            ("selfless.library.source", "radarr"),
        ])]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn retry_on_a_failed_delivery_processes_it_again() {
    let mut fixture = Fixture::start().await;
    fixture.media.create(ARRIVAL_FILE);
    fixture.radarr.fail_movie(ARRIVAL).await;
    let received_at = fixture
        .server
        .clock
        .now()
        .strftime("%Y-%m-%d %H:%M UTC")
        .to_string();
    fixture
        .server
        .send_radarr_webhook(
            Method::POST,
            Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
            &support::fixture("radarr/webhooks/download.json"),
        )
        .await;
    let failed = eventually(async || {
        fixture.server.clock.advance(SignedDuration::from_hours(1));
        application::process_inbox_row::call(&fixture.server.state)
            .await
            .unwrap();
        application::sync_status::call(&fixture.server.state)
            .await
            .unwrap()
            .failed_rows
            .pop()
    })
    .await;
    let (logs, _guard) = capture_logs();

    let page = fixture.sync_page().await;
    let deliveries = page.section_text("Failed deliveries");
    for expected in [
        "Radarr",
        "Download",
        "500",
        "5",
        received_at.as_str(),
        "Retry",
    ] {
        assert!(
            deliveries.contains(expected),
            "{expected:?} in {deliveries}"
        );
    }

    fixture
        .radarr
        .serve_movie(
            ARRIVAL,
            StatusCode::OK,
            &fixture.media.fixture("radarr/api/movie.json"),
        )
        .await;
    let retried = fixture.click(&page, "Retry").await;

    assert!(
        retried.text().contains("Retrying the delivery"),
        "{}",
        retried.body
    );
    assert!(
        retried
            .section_text("Failed deliveries")
            .contains("No failed deliveries")
    );
    eventually(async || {
        let page = fixture.sync_page().await;
        page.section_text("Radarr")
            .contains("1 movie")
            .then_some(())
    })
    .await;
    assert_eq!(
        admin_actions(&logs),
        [attributes([
            ("selfless.admin.action", "sync.retry"),
            ("selfless.admin.target.id", failed.id.to_string().as_str()),
        ])]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_delivery_opens_a_dialog_with_each_attempt_and_the_body_as_formatted_json() {
    let mut fixture = Fixture::start().await;
    fixture.radarr.fail_movie(ARRIVAL).await;
    let body = support::fixture("radarr/webhooks/download.json");
    let mut attempted_at = vec![fixture.server.clock.now()];
    fixture
        .server
        .send_radarr_webhook(
            Method::POST,
            Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
            &body.lines().map(str::trim).collect::<String>(),
        )
        .await;
    eventually(async || (fixture.radarr.movie_requests(ARRIVAL).await == 1).then_some(())).await;
    for delay in [1, 5, 15, 30] {
        if delay == 30 {
            fixture
                .radarr
                .serve_movie(ARRIVAL, StatusCode::SERVICE_UNAVAILABLE, "")
                .await;
        }
        fixture
            .server
            .clock
            .advance(SignedDuration::from_mins(delay));
        attempted_at.push(fixture.server.clock.now());
        application::process_inbox_row::call(&fixture.server.state)
            .await
            .unwrap();
    }
    let attempted_at: Vec<String> = attempted_at
        .iter()
        .map(|x| x.strftime("%Y-%m-%d %H:%M UTC").to_string())
        .collect();

    let dialog = fixture.sync_page().await.dialog_text("Download");

    assert_in_order(
        &dialog,
        &[
            "Last error",
            "503",
            "Attempts",
            &attempted_at[0],
            "500",
            &attempted_at[1],
            "500",
            &attempted_at[2],
            "500",
            &attempted_at[3],
            "500",
            &attempted_at[4],
            "503",
            "Body",
            body.trim_end(),
        ],
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_delivery_whose_body_is_not_json_shows_the_body_as_text() {
    let mut fixture = Fixture::start().await;
    fixture
        .server
        .send_radarr_webhook(
            Method::POST,
            Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
            "not json",
        )
        .await;
    eventually(async || {
        fixture.server.clock.advance(SignedDuration::from_hours(1));
        application::process_inbox_row::call(&fixture.server.state)
            .await
            .unwrap();
        application::sync_status::call(&fixture.server.state)
            .await
            .unwrap()
            .failed_rows
            .pop()
    })
    .await;

    let dialog = fixture.sync_page().await.dialog_text("Unknown");

    assert_in_order(&dialog, &["Body", "not json"]);
    fixture.stop().await;
}

#[tokio::test]
async fn sync_now_starts_a_reconcile_of_both_sources() {
    let mut fixture = Fixture::start().await;
    fixture.radarr_lists(&[ARRIVAL]).await;
    let (logs, _guard) = capture_logs();
    let page = fixture.sync_page().await;
    assert!(page.section_text("Radarr").contains("0 movies"));

    fixture.server.clock.advance(SignedDuration::from_mins(1));
    let started = fixture.click(&page, "Sync now").await;

    assert!(
        started.text().contains("Reconcile started"),
        "{}",
        started.body
    );
    assert!(started.body.contains(POLLING), "{}", started.body);
    fixture.reconciled().await;
    let page = fixture.sync_page().await;
    assert!(page.section_text("Radarr").contains("1 movie"));
    assert_eq!(
        admin_actions(&logs),
        [attributes([("selfless.admin.action", "sync.start")])]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn the_cards_poll_and_show_progress_only_while_a_reconcile_runs() {
    let mut fixture = Fixture::start().await;
    fixture.radarr_lists(&[ARRIVAL, DUNE]).await;
    let idle = fixture.sync_page().await;
    assert!(!idle.body.contains(POLLING), "{}", idle.body);

    fixture.server.prober.hold();
    fixture.server.clock.advance(SignedDuration::from_mins(1));
    fixture.click(&idle, "Sync now").await;

    let running = eventually(async || {
        let cards = fixture.browser.get(SOURCES_PATH).await;
        cards
            .section_text("Radarr")
            .contains("imported 0 of 2")
            .then_some(cards)
    })
    .await;
    assert!(running.body.contains(POLLING), "{}", running.body);
    assert!(fixture.sync_page().await.body.contains(POLLING));

    fixture.server.prober.release();
    fixture.reconciled().await;
    let finished = fixture.browser.get(SOURCES_PATH).await;
    let radarr = finished.section_text("Radarr");
    assert!(!radarr.contains("imported"), "{radarr}");
    assert!(radarr.contains("2 movies"), "{radarr}");
    assert!(!finished.body.contains(POLLING), "{}", finished.body);
    fixture.stop().await;
}

#[tokio::test]
async fn items_with_problems_lists_the_kind_the_item_and_the_path() {
    let mut fixture = Fixture::start().await;
    fixture
        .radarr
        .serve_movies(StatusCode::OK, &fixture.listed_movies(&[ARRIVAL]))
        .await;
    for file in [SEVERANCE_FILES[0], SEVERANCE_FILES[1], SEVERANCE_FILES[3]] {
        fixture.media.create(file);
    }
    fixture.sonarr_lists_severance().await;
    fixture.sync_now().await;

    let page = fixture.sync_page().await;

    let problems = page.section_text("Items with problems");
    let arrival_path = fixture.media.path(ARRIVAL_FILE);
    let severance_path = fixture.media.path(SEVERANCE_FILES[2]);
    for expected in [
        "File not found",
        "Arrival",
        arrival_path.as_str(),
        "Severance · S1 E2 · Half Loop",
        "Severance · S1 E3 · In Perpetuity",
        severance_path.as_str(),
    ] {
        assert!(problems.contains(expected), "{expected:?} in {problems}");
    }
    fixture.stop().await;
}
