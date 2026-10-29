#![allow(dead_code)]

use jsonschema::Registry;
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value;
use serde_json::json;
use settings::AppSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::OtlpSettings;
use settings::WebSettings;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use tokio_postgres::NoTls;
use tokio_util::sync::CancellationToken;
use url::Url;

const OPENAPI: &str = include_str!("../../openapi.yaml");
const OPENAPI_URI: &str = "urn:selfless:openapi";
const PROTOBUF: &str = "application/x-protobuf";

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
}

impl TestServer {
    pub async fn start(database_url: Url) -> Self {
        Self::start_with_otlp(database_url, OtlpSettings::default()).await
    }

    pub async fn start_with_collector(database_url: Url, collector_url: &str) -> Self {
        Self::start_with_otlp(
            database_url,
            OtlpSettings {
                endpoint: Some(collector_url.to_owned()),
                ..OtlpSettings::default()
            },
        )
        .await
    }

    async fn start_with_otlp(database_url: Url, otlp: OtlpSettings) -> Self {
        let server = selfless::start(AppSettings {
            http: HttpSettings { port: 0 },
            database: DatabaseSettings {
                url: database_url.to_string(),
            },
            web: WebSettings::default(),
            otlp,
        })
        .await
        .unwrap();
        let address = server.local_addr().unwrap();
        let shutdown = CancellationToken::new();
        tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
        Self { address, shutdown }
    }

    pub fn shut_down(&self) {
        self.shutdown.cancel();
    }

    pub async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = reqwest::get(format!("http://{}{path}", self.address))
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<Value>().await.unwrap();
        assert_matches_openapi(path, status, &body);
        (status, body)
    }

    pub async fn get_with_traceparent(&self, path: &str, traceparent: &str) -> (StatusCode, Value) {
        let response = reqwest::Client::new()
            .get(format!("http://{}{path}", self.address))
            .header("traceparent", traceparent)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.json::<Value>().await.unwrap();
        assert_matches_openapi(path, status, &body);
        (status, body)
    }

    pub async fn post_protobuf(&self, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
        let response = reqwest::Client::new()
            .post(format!("http://{}{path}", self.address))
            .header(CONTENT_TYPE, PROTOBUF)
            .body(body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let content_type = response.headers().get(CONTENT_TYPE).cloned();
        let body = response.bytes().await.unwrap().to_vec();
        let openapi: Value = serde_saphyr::from_str(OPENAPI).unwrap();
        let operation = format!("/paths/{}/post", path.replace('~', "~0").replace('/', "~1"));
        for pointer in [
            format!("{operation}/requestBody/content/application~1x-protobuf"),
            format!(
                "{operation}/responses/{}/content/application~1x-protobuf",
                status.as_u16()
            ),
        ] {
            assert!(
                openapi.pointer(&pointer).is_some(),
                "openapi.yaml has nothing at {pointer}"
            );
        }
        assert_eq!(
            content_type.as_ref().and_then(|x| x.to_str().ok()),
            Some(PROTOBUF)
        );
        (status, body)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

fn assert_matches_openapi(path: &str, status: StatusCode, body: &Value) {
    let openapi: Value = serde_saphyr::from_str(OPENAPI).unwrap();
    let pointer = format!(
        "/paths/{}/get/responses/{}/content/application~1json/schema",
        path.replace('~', "~0").replace('/', "~1"),
        status.as_u16()
    );
    assert!(
        openapi.pointer(&pointer).is_some(),
        "openapi.yaml has no schema at {pointer}"
    );
    let registry = Registry::new()
        .add(OPENAPI_URI, openapi)
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
        let target = format!(
            "{}:{}",
            target.host_str().unwrap(),
            target.port().unwrap_or(5432)
        );
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
