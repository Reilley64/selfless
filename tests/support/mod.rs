#![allow(dead_code)]

use application::ports::Clock;
use infrastructure::AppState;
use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::Unit;
use jsonschema::Registry;
use reqwest::Method;
use reqwest::RequestBuilder;
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use serde_json::json;
use settings::AppSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::LibrarySyncSettings;
use settings::OtlpSettings;
use settings::RadarrSettings;
use settings::SonarrSettings;
use settings::WebSettings;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tokio_postgres::NoTls;
use tokio_util::sync::CancellationToken;
use url::Url;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

static OPENAPI: LazyLock<Value> =
    LazyLock::new(|| serde_saphyr::from_str(include_str!("../../openapi.yaml")).unwrap());
const OPENAPI_URI: &str = "urn:selfless:openapi";
const PROTOBUF: &str = "application/x-protobuf";
pub const RADARR_WEBHOOK_PASSWORD: &str = "radarr webhook password";
pub const RADARR_API_KEY: &str = "radarr api key";
pub const SONARR_WEBHOOK_PASSWORD: &str = "sonarr webhook password";
pub const SONARR_API_KEY: &str = "sonarr api key";
const MEDIA_ROOT: &str = "/mnt/media";

pub struct TestDatabase {
    url: Url,
    name: String,
}

impl TestDatabase {
    pub async fn create() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!(
            "selfless_test_{nanos}_{}",
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        execute(&format!(r#"CREATE DATABASE "{name}""#)).await;
        let mut url = server_url();
        url.set_path(&name);
        Self { url, name }
    }

    pub fn url(&self) -> Url {
        self.url.clone()
    }

    pub async fn drop(self) {
        execute(&format!(r#"DROP DATABASE "{}" WITH (FORCE)"#, self.name)).await;
    }
}

fn server_url() -> Url {
    std::env::var("DATABASE__URL")
        .expect("DATABASE__URL points at the test PostgreSQL server")
        .parse()
        .unwrap()
}

async fn execute(sql: &str) {
    let (client, connection) = tokio_postgres::connect(server_url().as_str(), NoTls)
        .await
        .unwrap();
    let connection = tokio::spawn(connection);
    client.batch_execute(sql).await.unwrap();
    drop(client);
    connection.await.unwrap().unwrap();
}

pub struct TestServer {
    address: SocketAddr,
    shutdown: CancellationToken,
    pub state: AppState,
    pub clock: FakeClock,
}

pub fn settings(database_url: Url) -> AppSettings {
    AppSettings {
        http: HttpSettings { port: 0 },
        database: DatabaseSettings { url: database_url },
        web: WebSettings::default(),
        otlp: OtlpSettings::default(),
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

#[derive(Clone)]
pub struct FakeClock(Arc<Mutex<Timestamp>>);

impl FakeClock {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(
            Timestamp::now().round(Unit::Second).unwrap(),
        )))
    }

    pub fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }

    pub fn advance(&self, by: SignedDuration) {
        let mut now = self.0.lock().unwrap();
        *now += by;
    }

    pub fn port(&self) -> Clock {
        let now = self.0.clone();
        Clock::new(move || *now.lock().unwrap())
    }
}

impl TestServer {
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

    pub async fn start_with_collector(database_url: Url, collector_url: &str) -> Self {
        let mut settings = settings(database_url);
        settings.otlp.endpoint = Some(collector_url.to_owned());
        Self::start_with(settings, FakeClock::new()).await
    }

    pub async fn start_with(settings: AppSettings, clock: FakeClock) -> Self {
        let server = selfless::start(settings, clock.port()).await.unwrap();
        let address = server.local_addr().unwrap();
        let state = server.state().clone();
        let shutdown = CancellationToken::new();
        tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
        Self {
            address,
            shutdown,
            state,
            clock,
        }
    }

    pub fn shut_down(&self) {
        self.shutdown.cancel();
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        get_json(reqwest::Client::new().get(self.url(path)), path).await
    }

    pub async fn get_with_traceparent(&self, path: &str, traceparent: &str) -> (StatusCode, Value) {
        get_json(
            reqwest::Client::new()
                .get(self.url(path))
                .header("traceparent", traceparent),
            path,
        )
        .await
    }

    pub async fn get_text(&self, path: &str) -> (StatusCode, String) {
        let response = reqwest::get(self.url(path)).await.unwrap();
        (response.status(), response.text().await.unwrap())
    }

    pub async fn post_protobuf(&self, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
        let response = reqwest::Client::new()
            .post(self.url(path))
            .header(CONTENT_TYPE, PROTOBUF)
            .body(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let body = response.bytes().await.unwrap().to_vec();
        let operation = operation_pointer(path, "post");
        for pointer in [
            format!("{operation}/requestBody/content/application~1x-protobuf"),
            format!(
                "{operation}/responses/{}/content/application~1x-protobuf",
                status.as_u16()
            ),
        ] {
            assert!(
                OPENAPI.pointer(&pointer).is_some(),
                "openapi.yaml has nothing at {pointer}"
            );
        }
        assert_eq!(
            content_type.as_ref().and_then(|x| x.to_str().ok()),
            Some(PROTOBUF)
        );
        (status, body)
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
            .request(method, self.url(path))
            .header(CONTENT_TYPE, "application/json")
            .body(body.to_owned());
        if let Some((username, password)) = credentials {
            request = request.basic_auth(username, Some(password));
        }
        request.send().await.unwrap().status()
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn get_json(request: RequestBuilder, path: &str) -> (StatusCode, Value) {
    let response = request.send().await.unwrap();
    let status = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_matches_openapi(path, status, &body);
    (status, body)
}

fn operation_pointer(path: &str, method: &str) -> String {
    format!(
        "/paths/{}/{method}",
        path.replace('~', "~0").replace('/', "~1")
    )
}

fn assert_matches_openapi(path: &str, status: StatusCode, body: &Value) {
    let pointer = format!(
        "{}/responses/{}/content/application~1json/schema",
        operation_pointer(path, "get"),
        status.as_u16()
    );
    assert!(
        OPENAPI.pointer(&pointer).is_some(),
        "openapi.yaml has no schema at {pointer}"
    );
    let registry = Registry::new()
        .add(OPENAPI_URI, OPENAPI.clone())
        .unwrap()
        .prepare()
        .unwrap();
    let validator = jsonschema::options()
        .with_registry(&registry)
        .build(&json!({ "$ref": format!("{OPENAPI_URI}#{pointer}") }))
        .unwrap();
    let errors = validator
        .iter_errors(body)
        .map(|x| x.to_string())
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "GET {path} {status} does not match openapi.yaml: {errors:?}\n{body}"
    );
}

pub fn fixture(path: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{path}",
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

pub struct Proxy {
    address: SocketAddr,
    task: JoinHandle<()>,
}

impl Proxy {
    pub async fn start(target: &Url) -> Self {
        let location = DatabaseSettings {
            url: target.clone(),
        }
        .location();
        let target = format!("{}:{}", location.address, location.port);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                let (mut inbound, _) = listener.accept().await.unwrap();
                let target = target.clone();
                connections.spawn(async move {
                    let mut outbound = TcpStream::connect(target).await.unwrap();
                    let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                });
            }
        });
        Self { address, task }
    }

    pub fn url(&self, database_url: &Url) -> Url {
        let mut url = database_url.clone();
        url.set_host(Some(&self.address.ip().to_string())).unwrap();
        url.set_port(Some(self.address.port())).unwrap();
        url
    }

    pub fn stop(&self) {
        self.task.abort();
    }
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
