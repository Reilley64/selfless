#![allow(dead_code)]

use application::DbContext;
use application::ports::Clock;
use application::ports::Probe;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use infrastructure::AppState;
use infrastructure::persistence;
use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::Unit;
use jsonschema::Registry;
use reqwest::Method;
use reqwest::RequestBuilder;
use reqwest::StatusCode;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::COOKIE;
use reqwest::header::HeaderMap;
use reqwest::header::LOCATION;
use reqwest::header::ORIGIN;
use reqwest::header::SET_COOKIE;
use reqwest::redirect::Policy;
use serde_json::Value;
use serde_json::json;
use settings::AdminSettings;
use settings::AppSettings;
use settings::AssetSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::LibrarySyncSettings;
use settings::OtlpSettings;
use settings::RadarrSettings;
use settings::SmtpSettings;
use settings::SonarrSettings;
use settings::WebSettings;
use std::collections::BTreeMap;
use std::collections::HashMap;
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
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tokio_postgres::NoTls;
use tokio_postgres::Row;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;
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
pub const ADMIN_NAME: &str = "Maya";
pub const ADMIN_PASSWORD: &str = "correct horse battery staple";
pub const USER_NAME: &str = "Noor";
pub const USER_PASSWORD: &str = "a long passphrase about tea";
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

    pub async fn query(&self, sql: &str) -> Vec<Row> {
        let (client, connection) = tokio_postgres::connect(self.url.as_str(), NoTls)
            .await
            .unwrap();
        tokio::spawn(connection);
        client.query(sql, &[]).await.unwrap()
    }

    pub async fn admin_id(&self, name: &str) -> Uuid {
        let rows = self
            .query(&format!(
                "SELECT id::text FROM admins WHERE name = '{name}'"
            ))
            .await;
        rows[0].get::<_, &str>(0).parse().unwrap()
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentEmail {
    pub recipient: String,
    pub subject: String,
    pub body: String,
}

#[derive(Clone, Default)]
pub struct FakeMailer {
    sent: Arc<Mutex<Vec<SentEmail>>>,
}

impl FakeMailer {
    pub fn sent(&self) -> Vec<SentEmail> {
        self.sent.lock().unwrap().clone()
    }

    pub fn send_email(&self) -> SendEmail {
        let sent = self.sent.clone();
        SendEmail::new(move |recipient, subject, body| {
            sent.lock().unwrap().push(SentEmail {
                recipient,
                subject,
                body,
            });
            Box::pin(async { Ok(()) })
        })
    }
}

pub struct TestServer {
    address: SocketAddr,
    shutdown: CancellationToken,
    database: DatabaseSettings,
    pub mailer: FakeMailer,
    pub state: AppState,
    pub clock: FakeClock,
    pub prober: FakeProber,
}

pub fn settings(database_url: Url) -> AppSettings {
    AppSettings {
        http: HttpSettings { port: 0 },
        database: DatabaseSettings { url: database_url },
        web: WebSettings::default(),
        assets: AssetSettings {
            dir: Some(
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .and_then(|x| x.parent())
                    .unwrap()
                    .join("assets"),
            ),
        },
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

    pub fn advance(&self, duration: SignedDuration) {
        *self.0.lock().unwrap() += duration;
    }

    pub fn port(&self) -> Clock {
        let now = self.0.clone();
        Clock::new(move || *now.lock().unwrap())
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
        Self::start_with_prober(settings, clock, FakeProber::new()).await
    }

    pub async fn start_with_prober(
        settings: AppSettings,
        clock: FakeClock,
        prober: FakeProber,
    ) -> Self {
        let mailer = FakeMailer::default();
        let database = settings.database.clone();
        let server = selfless::start(settings, clock.port(), mailer.send_email(), prober.port())
            .await
            .unwrap();
        let address = server.local_addr().unwrap();
        let state = server.state().clone();
        let shutdown = CancellationToken::new();
        tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
        Self {
            address,
            shutdown,
            database,
            mailer,
            state,
            clock,
            prober,
        }
    }

    pub fn browser(&self) -> Browser {
        Browser {
            client: reqwest::Client::builder()
                .redirect(Policy::none())
                .build()
                .unwrap(),
            base: format!("http://{}", self.address),
            cookies: BTreeMap::new(),
        }
    }

    pub async fn db_context(&self) -> DbContext {
        DbContext::new(
            persistence::connect(&self.database).await.unwrap(),
            self.database.location(),
        )
    }

    pub async fn create_user(&self, name: &str, password: &str) -> Uuid {
        self.try_create_user(name, password).await.unwrap()
    }

    pub async fn try_create_user(
        &self,
        name: &str,
        password: &str,
    ) -> Result<Uuid, application::create_user::Error> {
        application::create_user::call(
            application::create_user::Dependencies {
                db_context: self.db_context().await,
                clock: self.clock.port(),
            },
            name.to_owned(),
            password.to_owned(),
            format!("{}@example.com", name.to_lowercase()),
        )
        .await
        .map(|x| x.id)
    }

    pub async fn sign_in(
        &self,
        name: &str,
        password: &str,
        client_kind: &str,
    ) -> (StatusCode, Value) {
        self.request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": name,
                "password": password,
                "clientKind": client_kind,
                "deviceName": "Living Room",
            })),
        )
        .await
    }

    pub async fn signed_in_user(&self) -> String {
        self.create_user(USER_NAME, USER_PASSWORD).await;
        let (status, body) = self.sign_in(USER_NAME, USER_PASSWORD, "web").await;
        assert_eq!(status, StatusCode::CREATED);
        body["token"].as_str().unwrap().to_owned()
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = reqwest::Client::new().request(method.clone(), self.url(path));
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let content_type = response.headers().get(CONTENT_TYPE).map(|x| {
            x.to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .trim()
                .to_owned()
        });
        let text = response.text().await.unwrap();
        let operation = operation_pointer(path, &method.as_str().to_lowercase());
        let response_pointer = format!("{operation}/responses/{}", status.as_u16());
        let response_pointer = match OPENAPI.pointer(&response_pointer) {
            Some(response) => response["$ref"]
                .as_str()
                .map_or(response_pointer, |x| x.trim_start_matches('#').to_owned()),
            None => panic!("openapi.yaml has nothing at {response_pointer}"),
        };
        let Some(content_type) = content_type.filter(|_| !text.is_empty()) else {
            assert!(
                OPENAPI
                    .pointer(&format!("{response_pointer}/content"))
                    .is_none(),
                "{method} {path} {status} has no body, but openapi.yaml describes one"
            );
            return (status, Value::Null);
        };
        let body = serde_json::from_str::<Value>(&text).unwrap();
        assert_matches_schema(
            &format!(
                "{response_pointer}/content/{}/schema",
                content_type.replace('~', "~0").replace('/', "~1")
            ),
            &format!("{method} {path} {status}"),
            &body,
        );
        (status, body)
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

    pub async fn get_text(&self, path: &str) -> (StatusCode, HeaderMap, String) {
        let response = reqwest::get(self.url(path)).await.unwrap();
        (
            response.status(),
            response.headers().clone(),
            response.text().await.unwrap(),
        )
    }

    pub async fn post_protobuf(
        &self,
        path: &str,
        token: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Vec<u8>) {
        let response = reqwest::Client::new()
            .post(self.url(path))
            .header(AUTHORIZATION, format!("Bearer {token}"))
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
    assert_matches_schema(
        &format!(
            "{}/responses/{}/content/application~1json/schema",
            operation_pointer(path, "get"),
            status.as_u16()
        ),
        &format!("GET {path} {status}"),
        body,
    );
}

fn assert_matches_schema(pointer: &str, response: &str, body: &Value) {
    assert!(
        OPENAPI.pointer(pointer).is_some(),
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
        "{response} does not match openapi.yaml: {errors:?}\n{body}"
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

pub struct Browser {
    client: reqwest::Client,
    base: String,
    cookies: BTreeMap<String, String>,
}

pub struct Page {
    pub status: StatusCode,
    pub location: Option<String>,
    pub set_cookies: Vec<String>,
    pub headers: HeaderMap,
    pub body: String,
}

impl Page {
    pub fn text(&self) -> String {
        visible_text(&self.body)
    }

    pub fn table_rows(&self) -> Vec<Vec<String>> {
        self.body
            .split("<tr")
            .skip(1)
            .map(|row| {
                row[..row.find("</tr>").unwrap()]
                    .split("<td")
                    .skip(1)
                    .map(|cell| {
                        let start = cell.find('>').unwrap() + 1;
                        visible_text(&cell[start..cell.find("</td>").unwrap()])
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|x| !x.is_empty())
            .collect()
    }

    pub fn link(&self, text: &str) -> String {
        let text_at = self
            .body
            .find(&format!(">{text}<"))
            .unwrap_or_else(|| panic!("the page has no {text:?} link\n{}", self.body));
        let link_at = self.body[..text_at]
            .rfind("<a ")
            .unwrap_or_else(|| panic!("{text:?} is not in a link\n{}", self.body));
        let start = link_at + self.body[link_at..].find(r#"href=""#).unwrap() + r#"href=""#.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].to_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|x| x.to_str().unwrap())
    }

    pub fn form_action(&self, button: &str) -> String {
        let button_at = self
            .body
            .find(&format!(">{button}<"))
            .unwrap_or_else(|| panic!("the page has no {button:?} button\n{}", self.body));
        let form_at = self.body[..button_at]
            .rfind("<form")
            .unwrap_or_else(|| panic!("{button:?} is not in a form\n{}", self.body));
        let start =
            form_at + self.body[form_at..].find(r#"action=""#).unwrap() + r#"action=""#.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].to_owned()
    }

    pub fn htmx_csrf_token(&self) -> String {
        let start = self
            .body
            .find(r#"hx-headers=""#)
            .unwrap_or_else(|| panic!("the page sets no htmx headers\n{}", self.body))
            + r#"hx-headers=""#.len();
        let length = self.body[start..].find('"').unwrap();
        let headers: Value =
            serde_json::from_str(&self.body[start..start + length].replace("&quot;", "\""))
                .unwrap();
        headers["X-CSRF-Token"].as_str().unwrap().to_owned()
    }

    pub fn csrf_token(&self) -> String {
        let start = self
            .body
            .find(r#"name="csrf" value=""#)
            .unwrap_or_else(|| panic!("the page has no CSRF field\n{}", self.body))
            + r#"name="csrf" value=""#.len();
        let length = self.body[start..].find('"').unwrap();
        self.body[start..start + length].to_owned()
    }
}

fn visible_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for char in html.chars() {
        match char {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(char),
            _ => {}
        }
    }
    text
}

impl Browser {
    pub async fn get(&mut self, path: &str) -> Page {
        let request = self.client.get(format!("{}{path}", self.base));
        self.send(request).await
    }

    pub async fn post_form(&mut self, path: &str, fields: &[(&str, &str)]) -> Page {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        let request = self
            .client
            .post(format!("{}{path}", self.base))
            .header(ORIGIN, &self.base)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(body);
        self.send(request).await
    }

    pub async fn sign_in(&mut self, name: &str, password: &str) -> Page {
        let form = self.get("/admin/sign-in").await;
        self.post_form(
            "/admin/sign-in",
            &[
                ("csrf", &form.csrf_token()),
                ("name", name),
                ("password", password),
            ],
        )
        .await
    }

    pub async fn htmx_post(&mut self, path: &str, csrf: Option<&str>) -> Page {
        let mut request = self
            .client
            .post(format!("{}{path}", self.base))
            .header("HX-Request", "true")
            .header(ORIGIN, &self.base)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body("");
        if let Some(csrf) = csrf {
            request = request.header("X-CSRF-Token", csrf);
        }
        self.send(request).await
    }

    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    pub fn set_cookie(&mut self, name: &str, value: &str) {
        self.cookies.insert(name.to_owned(), value.to_owned());
    }

    async fn send(&mut self, mut request: RequestBuilder) -> Page {
        if !self.cookies.is_empty() {
            let header = self
                .cookies
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; ");
            request = request.header(COOKIE, header);
        }
        let response = request.send().await.unwrap();
        let set_cookies = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .map(|x| x.to_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        for set_cookie in &set_cookies {
            let (pair, attributes) = set_cookie.split_once(';').unwrap_or((set_cookie, ""));
            let (name, value) = pair.split_once('=').unwrap();
            if attributes.contains("Max-Age=0") {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), value.to_owned());
            }
        }
        Page {
            status: response.status(),
            location: response
                .headers()
                .get(LOCATION)
                .map(|x| x.to_str().unwrap().to_owned()),
            set_cookies,
            headers: response.headers().clone(),
            body: response.text().await.unwrap(),
        }
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
