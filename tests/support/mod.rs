#![allow(dead_code)]

use application::ports::Clock;
use application::ports::SendEmail;
use jiff::SignedDuration;
use jiff::Timestamp;
use jsonschema::Registry;
use reqwest::RequestBuilder;
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::COOKIE;
use reqwest::header::HeaderMap;
use reqwest::header::LOCATION;
use reqwest::header::SET_COOKIE;
use reqwest::redirect::Policy;
use serde_json::Value;
use serde_json::json;
use settings::AdminSettings;
use settings::AppSettings;
use settings::AssetSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::OtlpSettings;
use settings::SmtpSettings;
use settings::WebSettings;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tokio_postgres::NoTls;
use tokio_postgres::Row;
use tokio_util::sync::CancellationToken;
use url::Url;

static OPENAPI: LazyLock<Value> =
    LazyLock::new(|| serde_saphyr::from_str(include_str!("../../openapi.yaml")).unwrap());
const OPENAPI_URI: &str = "urn:selfless:openapi";
const PROTOBUF: &str = "application/x-protobuf";
pub const ADMIN_NAME: &str = "Maya";
pub const ADMIN_PASSWORD: &str = "correct horse battery staple";

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
    pub mailer: FakeMailer,
    pub clock: FakeClock,
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
    }
}

#[derive(Clone)]
pub struct FakeClock(Arc<Mutex<Timestamp>>);

impl FakeClock {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Timestamp::now())))
    }

    pub fn advance(&self, duration: SignedDuration) {
        *self.0.lock().unwrap() += duration;
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

    pub async fn start_with_collector(database_url: Url, collector_url: &str) -> Self {
        let mut settings = settings(database_url);
        settings.otlp.endpoint = Some(collector_url.to_owned());
        Self::start_with(settings, FakeClock::new()).await
    }

    pub async fn start_with(settings: AppSettings, clock: FakeClock) -> Self {
        let mailer = FakeMailer::default();
        let server = selfless::start(settings, clock.port(), mailer.send_email())
            .await
            .unwrap();
        let address = server.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
        Self {
            address,
            shutdown,
            mailer,
            clock,
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
        let mut text = String::new();
        let mut in_tag = false;
        for char in self.body.chars() {
            match char {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => text.push(char),
                _ => {}
            }
        }
        text
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
