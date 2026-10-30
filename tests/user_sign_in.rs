mod support;

use application::sessions::sign_in;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::ClientKind;
use jiff::SignedDuration;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::FakeClock;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;
use tokio::time::Instant;
use uuid::Uuid;

const UNAUTHENTICATED: &str = "urn:selfless:problem:unauthenticated";

async fn sign_in(server: &TestServer, name: &str, password: &str) -> (StatusCode, Value) {
    server.sign_in(name, password, "web").await
}

async fn me(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(Method::GET, "/api/me/v1", Some(token), None)
        .await
        .0
}

#[tokio::test]
async fn a_user_signs_in_with_their_name_and_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let (status, body) = sign_in(&server, NAME, PASSWORD).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user"]["name"], NAME);
    let id = Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(id.get_version_num(), 7);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn user_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let (status, body) = sign_in(&server, &NAME.to_uppercase(), PASSWORD).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["user"]["name"], NAME);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_name_and_a_wrong_password_return_the_same_problem() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let wrong_name = sign_in(&server, "Maya", PASSWORD).await;
    let wrong_password = sign_in(&server, NAME, "wrong").await;

    assert_eq!(wrong_name.0, StatusCode::UNAUTHORIZED);
    assert_eq!(
        wrong_name.1["type"],
        "urn:selfless:problem:wrong-name-or-password"
    );
    assert_eq!(wrong_name, wrong_password);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_unknown_name_takes_as_long_as_a_wrong_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let sign_in_time = async |name: &str| {
        let start = Instant::now();
        sign_in(&server, name, "wrong").await;
        start.elapsed()
    };

    let wrong_password = sign_in_time(NAME).await;
    let unknown_name = sign_in_time("Maya").await;

    assert!(
        unknown_name * 2 > wrong_password,
        "unknown name {unknown_name:?}, wrong password {wrong_password:?}"
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_token_is_32_random_bytes_and_the_database_stores_only_its_sha_256_hash() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;

    let (_, first) = sign_in(&server, NAME, PASSWORD).await;
    let (_, second) = sign_in(&server, NAME, PASSWORD).await;

    let tokens = [&first, &second].map(|x| x["token"].as_str().unwrap().to_owned());
    assert_ne!(tokens[0], tokens[1]);
    for token in &tokens {
        assert_eq!(URL_SAFE_NO_PAD.decode(token).unwrap().len(), 32);
    }
    let stored = database
        .query("SELECT token_hash FROM sessions")
        .await
        .iter()
        .map(|x| x.get::<_, Vec<u8>>("token_hash"))
        .collect::<Vec<_>>();
    assert_eq!(
        stored,
        tokens.map(|x| Sha256::digest(x.as_bytes()).to_vec())
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn me_returns_the_signed_in_user() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, signed_in) = sign_in(&server, NAME, PASSWORD).await;

    let (status, me) = server
        .request(Method::GET, "/api/me/v1", signed_in["token"].as_str(), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(me, signed_in["user"]);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_missing_or_unknown_token_returns_401() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let missing = server.request(Method::GET, "/api/me/v1", None, None).await;
    let unknown = server
        .request(
            Method::GET,
            "/api/me/v1",
            Some("q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M"),
            None,
        )
        .await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.1["type"], UNAUTHENTICATED);
    assert_eq!(missing, unknown);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_out_ends_the_calling_session_only() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, first) = sign_in(&server, NAME, PASSWORD).await;
    let (_, second) = sign_in(&server, NAME, PASSWORD).await;
    let [first, second] = [&first, &second].map(|x| x["token"].as_str().unwrap().to_owned());

    let (status, _) = server
        .request(
            Method::DELETE,
            "/api/sessions/current/v1",
            Some(&first),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(me(&server, &first).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &second).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_web_session_ends_after_30_days_without_use() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, signed_in) = server.sign_in(NAME, PASSWORD, "web").await;
    let token = signed_in["token"].as_str().unwrap();

    server.clock.advance(SignedDuration::from_hours(29 * 24));
    let after_29_days = me(&server, token).await;
    server.clock.advance(SignedDuration::from_hours(29 * 24));
    let after_29_more_days = me(&server, token).await;
    server.clock.advance(SignedDuration::from_hours(30 * 24));
    let after_30_idle_days = me(&server, token).await;

    assert_eq!(after_29_days, StatusCode::OK);
    assert_eq!(after_29_more_days, StatusCode::OK);
    assert_eq!(after_30_idle_days, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn tv_sessions_never_expire_from_disuse() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, apple_tv) = server.sign_in(NAME, PASSWORD, "appleTv").await;
    let (_, google_tv) = server.sign_in(NAME, PASSWORD, "googleTv").await;

    server.clock.advance(SignedDuration::from_hours(400 * 24));

    for signed_in in [apple_tv, google_tv] {
        assert_eq!(
            me(&server, signed_in["token"].as_str().unwrap()).await,
            StatusCode::OK
        );
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn last_used_is_written_at_most_once_an_hour() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, signed_in) = sign_in(&server, NAME, PASSWORD).await;
    let token = signed_in["token"].as_str().unwrap();
    let last_used = async || {
        database
            .query("SELECT extract(epoch FROM last_used_at)::float8 AS last_used FROM sessions")
            .await[0]
            .get::<_, f64>("last_used")
    };
    let signed_in_at = last_used().await;

    server.clock.advance(SignedDuration::from_mins(59));
    me(&server, token).await;
    let after_59_minutes = last_used().await;
    server.clock.advance(SignedDuration::from_mins(1));
    me(&server, token).await;
    let after_an_hour = last_used().await;

    assert_eq!(after_59_minutes, signed_in_at);
    assert_eq!(after_an_hour - signed_in_at, 3600.0);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_user_token_is_rejected_on_admin() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let token = server.signed_in_user().await;
    let mut browser = server.browser();

    browser.set_cookie("session", &token);
    let home = browser.get("/admin").await;

    assert_eq!(home.status, StatusCode::SEE_OTHER);
    assert_eq!(home.location.as_deref(), Some("/admin/sign-in"));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_admin_session_is_rejected_on_the_api() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    let admin_token = browser.cookie("session").unwrap().to_owned();

    let with_cookie = browser.get("/api/me/v1").await;
    let with_bearer = server
        .request(Method::GET, "/api/me/v1", Some(&admin_token), None)
        .await;

    assert_eq!(with_cookie.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        serde_json::from_str::<Value>(&with_cookie.body).unwrap()["type"],
        UNAUTHENTICATED
    );
    assert_eq!(with_bearer.0, StatusCode::UNAUTHORIZED);
    assert_eq!(with_bearer.1["type"], UNAUTHENTICATED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_actuator_endpoints_and_the_web_app_stay_public() {
    let database = TestDatabase::create().await;
    let web_dir = std::env::temp_dir().join(format!("selfless-web-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&web_dir).unwrap();
    std::fs::write(web_dir.join("index.html"), "<title>selfless</title>").unwrap();
    std::fs::write(web_dir.join("app.js"), "export {};").unwrap();
    let mut settings = support::settings(database.url());
    settings.web.dir = web_dir.clone();
    let server = TestServer::start_with(settings, FakeClock::new()).await;

    let (info, _) = server.get("/actuator/info/v1").await;
    let (health, _) = server.get("/actuator/health/v1").await;
    let (index, _, html) = server.get_text("/").await;
    let (script, _, _) = server.get_text("/app.js").await;

    assert_eq!(info, StatusCode::OK);
    assert_eq!(health, StatusCode::OK);
    assert_eq!(index, StatusCode::OK);
    assert!(html.contains("<title>selfless</title>"), "{html}");
    assert_eq!(script, StatusCode::OK);
    drop(server);
    std::fs::remove_dir_all(web_dir).unwrap();
    database.drop().await;
}

#[tokio::test]
async fn a_sign_in_body_without_a_field_or_with_an_unknown_client_kind_returns_a_bad_request_problem()
 {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    for body in [
        json!({ "name": NAME, "password": PASSWORD, "deviceName": "Living Room" }),
        json!({ "name": NAME, "password": PASSWORD, "clientKind": "toaster", "deviceName": "Kitchen" }),
    ] {
        let (status, problem) = server
            .request(Method::POST, "/api/sessions/v1", None, Some(body))
            .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(problem["type"], "urn:selfless:problem:bad-request");
    }
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn signing_in_with_replaces_ends_the_old_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let (_, old) = server.sign_in(NAME, PASSWORD, "appleTv").await;
    let old = old["token"].as_str().unwrap();

    let (status, new) = server
        .request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": NAME,
                "password": PASSWORD,
                "clientKind": "appleTv",
                "deviceName": "Living Room",
                "replaces": [old],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(me(&server, old).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me(&server, new["token"].as_str().unwrap()).await,
        StatusCode::OK
    );
    assert_eq!(database.query("SELECT 1 FROM sessions").await.len(), 1);
    drop(server);
    database.drop().await;
}

async fn sign_in_on_tv(
    server: &TestServer,
    name: &str,
    password: &str,
    replaces: Vec<String>,
) -> sign_in::Output {
    sign_in::call(
        sign_in::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
        },
        name.to_owned(),
        password.to_owned(),
        ClientKind::AppleTv,
        "Living Room".to_owned(),
        replaces,
    )
    .await
    .unwrap()
}

fn token(output: sign_in::Output) -> String {
    match output {
        sign_in::Output::SignedIn { token, .. } => token,
        sign_in::Output::WrongNameOrPassword => panic!("wrong name or password"),
    }
}

#[tokio::test]
async fn a_replaces_token_of_another_user_is_ignored() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    server.create_user("Theo", "battery staple").await;
    let theo = token(sign_in_on_tv(&server, "Theo", "battery staple", vec![]).await);
    let unknown = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M".to_owned();

    let output = sign_in_on_tv(&server, NAME, PASSWORD, vec![theo.clone(), unknown]).await;

    assert_eq!(me(&server, &token(output)).await, StatusCode::OK);
    assert_eq!(me(&server, &theo).await, StatusCode::OK);
    assert_eq!(database.query("SELECT 1 FROM sessions").await.len(), 2);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_password_with_replaces_ends_nothing() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let old = token(sign_in_on_tv(&server, NAME, PASSWORD, vec![]).await);

    let output = sign_in_on_tv(&server, NAME, "wrong", vec![old.clone()]).await;

    assert!(matches!(output, sign_in::Output::WrongNameOrPassword));
    assert_eq!(me(&server, &old).await, StatusCode::OK);
    assert_eq!(database.query("SELECT 1 FROM sessions").await.len(), 1);
    drop(server);
    database.drop().await;
}
