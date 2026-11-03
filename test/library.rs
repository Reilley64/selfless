#![allow(dead_code)]

#[path = "selfless.rs"]
pub mod selfless_test;

use application::ports::Clock;
use application::ports::Probe;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use infrastructure::AppState;
use jiff::Timestamp;
use reqwest::Method;
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use settings::AppSettings;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::TempDir;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

use selfless_test::RADARR_API_KEY;
use selfless_test::SONARR_API_KEY;
use selfless_test::settings;

const MEDIA_ROOT: &str = "/mnt/media";

pub struct LibraryServer {
    address: SocketAddr,
    shutdown: CancellationToken,
    pub state: AppState,
    pub clock: FakeClock,
    pub prober: FakeProber,
}

impl LibraryServer {
    pub async fn start(database_url: Url) -> Self {
        Self::start_with(settings(database_url), FakeClock::new()).await
    }

    pub async fn start_with_radarr(database_url: Url, radarr: &StubRadarr) -> Self {
        let mut settings = settings(database_url);
        settings.radarr.url = Some(radarr.url());
        Self::start_with(settings, FakeClock::new()).await
    }

    pub async fn start_with_sonarr(database_url: Url, sonarr: &StubSonarr) -> Self {
        let mut settings = settings(database_url);
        settings.sonarr.url = Some(sonarr.url());
        Self::start_with(settings, FakeClock::new()).await
    }

    pub async fn start_with(settings: AppSettings, clock: FakeClock) -> Self {
        Self::start_with_prober(settings, clock, FakeProber::new()).await
    }

    pub async fn start_with_prober(
        settings: AppSettings,
        clock: FakeClock,
        prober: FakeProber,
    ) -> Self {
        let server = selfless::start(
            settings,
            Clock::new(clock.reader()),
            SendEmail::new(FakeMailer::default().sender()),
            prober.port(),
        )
        .await
        .unwrap();
        let address = server.local_addr().unwrap();
        let state = server.state().clone();
        let shutdown = CancellationToken::new();
        tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
        Self {
            address,
            shutdown,
            state,
            clock,
            prober,
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub async fn send_radarr_webhook(
        &self,
        method: Method,
        credentials: Option<(&str, &str)>,
        body: &str,
    ) -> StatusCode {
        self.send_webhook(method, "/webhooks/radarr/v1", credentials, body)
            .await
    }

    pub async fn send_sonarr_webhook(
        &self,
        method: Method,
        credentials: Option<(&str, &str)>,
        body: &str,
    ) -> StatusCode {
        self.send_webhook(method, "/webhooks/sonarr/v1", credentials, body)
            .await
    }

    pub async fn send_webhook(
        &self,
        method: Method,
        path: &str,
        credentials: Option<(&str, &str)>,
        body: &str,
    ) -> StatusCode {
        let mut request = reqwest::Client::new()
            .request(method, format!("{}{path}", self.base_url()))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_owned());
        if let Some((username, password)) = credentials {
            request = request.basic_auth(username, Some(password));
        }
        request.send().await.unwrap().status()
    }
}

impl Drop for LibraryServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

#[derive(Clone)]
pub struct FakeProber {
    state: Arc<Mutex<ProberState>>,
    held: watch::Sender<bool>,
}

#[derive(Default)]
struct ProberState {
    probes: HashMap<String, Option<Probe>>,
    calls: HashMap<String, usize>,
    running: usize,
    most_running: usize,
}

impl FakeProber {
    pub fn new() -> Self {
        Self {
            state: Arc::default(),
            held: watch::Sender::new(false),
        }
    }

    pub fn serve(&self, path: &str, probe: Probe) {
        self.state
            .lock()
            .unwrap()
            .probes
            .insert(path.to_owned(), Some(probe));
    }

    pub fn fail(&self, path: &str) {
        self.state
            .lock()
            .unwrap()
            .probes
            .insert(path.to_owned(), None);
    }

    pub fn calls(&self, path: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .calls
            .get(path)
            .copied()
            .unwrap_or_default()
    }

    pub fn total_calls(&self) -> usize {
        self.state.lock().unwrap().calls.values().sum()
    }

    pub fn running(&self) -> usize {
        self.state.lock().unwrap().running
    }

    pub fn most_running(&self) -> usize {
        self.state.lock().unwrap().most_running
    }

    pub fn hold(&self) {
        self.held.send_replace(true);
    }

    pub fn release(&self) {
        self.held.send_replace(false);
    }

    pub fn port(&self) -> ProbeMediaFile {
        let prober = self.clone();
        ProbeMediaFile::new(move |path| {
            let prober = prober.clone();
            Box::pin(async move {
                {
                    let mut state = prober.state.lock().unwrap();
                    *state.calls.entry(path.clone()).or_default() += 1;
                    state.running += 1;
                    state.most_running = state.most_running.max(state.running);
                }
                let _ = prober.held.subscribe().wait_for(|held| !held).await;
                let mut state = prober.state.lock().unwrap();
                state.running -= 1;
                match state.probes.get(&path) {
                    Some(Some(probe)) => Ok(probe.clone()),
                    Some(None) => Err(ProbeError::caused_by(
                        "Invalid data found when processing input",
                    )),
                    None => Ok(Probe {
                        container: "matroska,webm".to_owned(),
                        duration: None,
                        video_streams: Vec::new(),
                        audio_streams: Vec::new(),
                        subtitle_streams: Vec::new(),
                    }),
                }
            })
        })
    }
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
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the condition did not hold within 5 seconds");
}

pub struct StubRadarr(MockServer);

impl StubRadarr {
    pub async fn start() -> Self {
        Self(MockServer::start().await)
    }

    pub fn url(&self) -> Url {
        self.0.uri().parse().unwrap()
    }

    pub async fn serve_movie(&self, id: i64, status: StatusCode, body: &str) {
        self.0.reset().await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/movie/{id}")))
            .and(header("X-Api-Key", RADARR_API_KEY))
            .respond_with(
                ResponseTemplate::new(status.as_u16())
                    .set_body_raw(body.to_owned(), "application/json"),
            )
            .mount(&self.0)
            .await;
    }

    pub async fn fail_movie(&self, id: i64) {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/movie/{id}")))
            .respond_with(ResponseTemplate::new(
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ))
            .mount(&self.0)
            .await;
    }

    pub async fn movie_requests(&self, id: i64) -> usize {
        let movie_path = format!("/api/v3/movie/{id}");
        self.0
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|x| x.url.path() == movie_path)
            .count()
    }

    pub async fn serve_metadata(&self, body: &str) {
        Mock::given(method("GET"))
            .and(path("/api/v3/metadata"))
            .and(header("X-Api-Key", RADARR_API_KEY))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16())
                    .set_body_raw(body.to_owned(), "application/json"),
            )
            .mount(&self.0)
            .await;
    }

    pub async fn fail_metadata(&self) {
        Mock::given(method("GET"))
            .and(path("/api/v3/metadata"))
            .respond_with(ResponseTemplate::new(
                StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            ))
            .mount(&self.0)
            .await;
    }

    pub async fn serve_movies(&self, status: StatusCode, body: &str) {
        self.0.reset().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/movie"))
            .and(header("X-Api-Key", RADARR_API_KEY))
            .respond_with(
                ResponseTemplate::new(status.as_u16())
                    .set_body_raw(body.to_owned(), "application/json"),
            )
            .mount(&self.0)
            .await;
        default_metadata(RADARR_API_KEY, &fixture("radarr/api/metadata.json"))
            .mount(&self.0)
            .await;
    }
}

pub struct StubSonarr(MockServer);

impl StubSonarr {
    pub async fn start() -> Self {
        Self(MockServer::start().await)
    }

    pub fn url(&self) -> Url {
        self.0.uri().parse().unwrap()
    }

    pub async fn serve_series(&self, id: i64, series: &str, episodes: &str, episode_files: &str) {
        self.0.reset().await;
        self.mount(
            &format!("/api/v3/series/{id}"),
            None,
            StatusCode::OK,
            series,
        )
        .await;
        self.mount("/api/v3/episode", Some(id), StatusCode::OK, episodes)
            .await;
        self.mount(
            "/api/v3/episodefile",
            Some(id),
            StatusCode::OK,
            episode_files,
        )
        .await;
    }

    pub async fn forget_series(&self, id: i64) {
        self.0.reset().await;
        self.mount(
            &format!("/api/v3/series/{id}"),
            None,
            StatusCode::NOT_FOUND,
            &fixture("sonarr/api/series-not-found.json"),
        )
        .await;
    }

    pub async fn serve_series_list(&self, series: &str) {
        self.0.reset().await;
        self.mount("/api/v3/series", None, StatusCode::OK, series)
            .await;
        default_metadata(SONARR_API_KEY, &fixture("sonarr/api/metadata.json"))
            .mount(&self.0)
            .await;
    }

    pub async fn serve_episodes(
        &self,
        series_id: i64,
        episodes: &str,
        episode_files: &str,
        delay: Duration,
    ) {
        for (url_path, body) in [
            ("/api/v3/episode", episodes),
            ("/api/v3/episodefile", episode_files),
        ] {
            Mock::given(method("GET"))
                .and(path(url_path))
                .and(header("X-Api-Key", SONARR_API_KEY))
                .and(query_param("seriesId", series_id.to_string()))
                .respond_with(
                    ResponseTemplate::new(StatusCode::OK.as_u16())
                        .set_body_raw(body.to_owned(), "application/json")
                        .set_delay(delay),
                )
                .mount(&self.0)
                .await;
        }
    }

    pub async fn serve_metadata(&self, body: &str) {
        Mock::given(method("GET"))
            .and(path("/api/v3/metadata"))
            .and(header("X-Api-Key", SONARR_API_KEY))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16())
                    .set_body_raw(body.to_owned(), "application/json"),
            )
            .mount(&self.0)
            .await;
    }

    pub async fn fail_episodes(&self, series_id: i64) {
        self.mount(
            "/api/v3/episode",
            Some(series_id),
            StatusCode::INTERNAL_SERVER_ERROR,
            "{}",
        )
        .await;
    }

    async fn mount(&self, url_path: &str, series_id: Option<i64>, status: StatusCode, body: &str) {
        let mut mock = Mock::given(method("GET"))
            .and(path(url_path))
            .and(header("X-Api-Key", SONARR_API_KEY));
        if let Some(series_id) = series_id {
            mock = mock.and(query_param("seriesId", series_id.to_string()));
        }
        mock.respond_with(
            ResponseTemplate::new(status.as_u16())
                .set_body_raw(body.to_owned(), "application/json"),
        )
        .mount(&self.0)
        .await;
    }
}

fn default_metadata(api_key: &str, body: &str) -> Mock {
    Mock::given(method("GET"))
        .and(path("/api/v3/metadata"))
        .and(header("X-Api-Key", api_key))
        .respond_with(
            ResponseTemplate::new(StatusCode::OK.as_u16())
                .set_body_raw(body.to_owned(), "application/json"),
        )
        .with_priority(10)
}

pub struct MediaFolder(TempDir);

impl MediaFolder {
    pub fn new() -> Self {
        Self(TempDir::new().unwrap())
    }

    pub fn create(&self, relative_path: &str) {
        let path = self.0.path().join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, []).unwrap();
    }

    pub fn create_absolute(&self, path: &str) {
        self.create(
            Path::new(path)
                .strip_prefix(self.0.path())
                .unwrap()
                .to_str()
                .unwrap(),
        );
    }

    pub fn remove_absolute(&self, path: &str) {
        std::fs::remove_file(path).unwrap();
    }

    pub fn change_absolute(&self, path: &str) {
        std::fs::write(path, "changed").unwrap();
    }

    pub fn write(&self, relative_path: &str, contents: &str) {
        let path = self.0.path().join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub fn set_modified(&self, relative_path: &str, modified_at: &str) {
        std::fs::File::options()
            .write(true)
            .open(self.0.path().join(relative_path))
            .unwrap()
            .set_modified(SystemTime::from(modified_at.parse::<Timestamp>().unwrap()))
            .unwrap();
    }

    pub fn remove(&self, relative_path: &str) {
        std::fs::remove_file(self.0.path().join(relative_path)).unwrap();
    }

    pub fn path(&self, relative_path: &str) -> String {
        self.0
            .path()
            .join(relative_path)
            .to_str()
            .unwrap()
            .to_owned()
    }

    pub fn fixture(&self, path: &str) -> String {
        fixture(path).replace(MEDIA_ROOT, &self.0.path().to_string_lossy())
    }
}
