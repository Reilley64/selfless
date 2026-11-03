use application::ports::Clock;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use settings::AdminSettings;
use settings::AppSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::LibrarySyncSettings;
use settings::OtlpSettings;
use settings::RadarrSettings;
use settings::SmtpSettings;
use settings::SonarrSettings;
use settings::WebSettings;
use std::time::Duration;
use tempfile::TempDir;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestServer;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

pub const ADMIN_NAME: &str = "Maya";
pub const ADMIN_PASSWORD: &str = "correct horse battery staple";
pub const RADARR_WEBHOOK_PASSWORD: &str = "radarr webhook password";
pub const RADARR_API_KEY: &str = "radarr api key";
pub const SONARR_WEBHOOK_PASSWORD: &str = "sonarr webhook password";
pub const SONARR_API_KEY: &str = "sonarr api key";

const MEDIA_ROOT: &str = "/mnt/media";

pub fn settings(database_url: Url) -> AppSettings {
    AppSettings {
        http: HttpSettings { port: 0 },
        database: DatabaseSettings { url: database_url },
        web: WebSettings::default(),
        admin: AdminSettings {
            username: Some(ADMIN_NAME.to_owned()),
            password: Some(ADMIN_PASSWORD.to_owned()),
        },
        otlp: OtlpSettings::default(),
        smtp: SmtpSettings::default(),
        radarr: RadarrSettings {
            url: Some("http://127.0.0.1:1".parse().unwrap()),
            api_key: Some(RADARR_API_KEY.to_owned()),
            webhook_password: Some(RADARR_WEBHOOK_PASSWORD.to_owned()),
        },
        sonarr: SonarrSettings {
            url: Some("http://127.0.0.1:1".parse().unwrap()),
            api_key: Some(SONARR_API_KEY.to_owned()),
            webhook_password: Some(SONARR_WEBHOOK_PASSWORD.to_owned()),
        },
        library_sync: LibrarySyncSettings::default(),
    }
}

pub async fn start(settings: AppSettings, clock: &FakeClock, mailer: &FakeMailer) -> TestServer {
    let server = selfless::start(
        settings,
        Clock::new(clock.reader()),
        SendEmail::new(mailer.sender()),
        ProbeMediaFile::new(|_| Box::pin(async { Err(ProbeError::default()) })),
    )
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
    TestServer::new(address, shutdown)
}

pub async fn send_webhook(
    server: &TestServer,
    path: &str,
    password: &str,
    mut headers: HeaderMap,
    body: &str,
) -> StatusCode {
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
    status
}

pub fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/test/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

pub async fn eventually<T>(mut check: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..100 {
        if let Some(value) = check().await {
            return value;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the condition did not hold within 5 seconds");
}

pub struct MediaFolder(TempDir);

impl Default for MediaFolder {
    fn default() -> Self {
        Self(TempDir::new().unwrap())
    }
}

impl MediaFolder {
    pub fn create(&self, relative_path: &str) {
        let path = self.0.path().join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, []).unwrap();
    }

    pub fn fixture(&self, path: &str) -> String {
        fixture(path).replace(MEDIA_ROOT, &self.0.path().to_string_lossy())
    }
}

pub struct StubRadarr(MockServer);

impl StubRadarr {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        metadata(RADARR_API_KEY, &fixture("radarr/api/metadata.json"))
            .mount(&server)
            .await;
        Self(server)
    }

    pub fn url(&self) -> Url {
        self.0.uri().parse().unwrap()
    }

    pub async fn serve_movie(&self, id: i64, body: &str) {
        respond(RADARR_API_KEY, &format!("/api/v3/movie/{id}"), None, body)
            .mount(&self.0)
            .await;
    }

    pub async fn serve_movies(&self, body: &str) {
        respond(RADARR_API_KEY, "/api/v3/movie", None, body)
            .mount(&self.0)
            .await;
    }
}

pub struct StubSonarr(MockServer);

impl StubSonarr {
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        metadata(SONARR_API_KEY, &fixture("sonarr/api/metadata.json"))
            .mount(&server)
            .await;
        Self(server)
    }

    pub fn url(&self) -> Url {
        self.0.uri().parse().unwrap()
    }

    pub async fn serve_series(&self, id: i64, series: &str, episodes: &str, episode_files: &str) {
        respond(
            SONARR_API_KEY,
            &format!("/api/v3/series/{id}"),
            None,
            series,
        )
        .mount(&self.0)
        .await;
        self.serve_episodes(id, episodes, episode_files).await;
    }

    pub async fn serve_series_list(&self, series: &str) {
        respond(SONARR_API_KEY, "/api/v3/series", None, series)
            .mount(&self.0)
            .await;
    }

    pub async fn serve_episodes(&self, series_id: i64, episodes: &str, episode_files: &str) {
        respond(SONARR_API_KEY, "/api/v3/episode", Some(series_id), episodes)
            .mount(&self.0)
            .await;
        respond(
            SONARR_API_KEY,
            "/api/v3/episodefile",
            Some(series_id),
            episode_files,
        )
        .mount(&self.0)
        .await;
    }
}

fn respond(api_key: &str, url_path: &str, series_id: Option<i64>, body: &str) -> Mock {
    let mut mock = Mock::given(method("GET"))
        .and(path(url_path))
        .and(header("X-Api-Key", api_key));
    if let Some(series_id) = series_id {
        mock = mock.and(query_param("seriesId", series_id.to_string()));
    }
    mock.respond_with(
        ResponseTemplate::new(StatusCode::OK.as_u16())
            .set_body_raw(body.to_owned(), "application/json"),
    )
}

fn metadata(api_key: &str, body: &str) -> Mock {
    respond(api_key, "/api/v3/metadata", None, body)
}
