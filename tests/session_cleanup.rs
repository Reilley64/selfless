mod support;

use jiff::SignedDuration;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::json;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::Browser;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME;
use support::USER_PASSWORD;
use uuid::Uuid;

async fn sign_in(server: &TestServer, client_kind: &str, device_name: &str) {
    let (status, _) = server
        .request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": USER_NAME,
                "password": USER_PASSWORD,
                "clientKind": client_kind,
                "deviceName": device_name,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
}

async fn sign_in_admin(server: &TestServer) -> Browser {
    let mut browser = server.browser();
    let page = browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    browser
}

async fn device_names(server: &TestServer, user_id: Uuid) -> Vec<String> {
    application::list_user_sessions::call(
        application::list_user_sessions::Dependencies {
            db_context: server.db_context().await,
        },
        user_id,
    )
    .await
    .unwrap()
    .into_iter()
    .map(|x| x.device_name)
    .collect()
}

async fn admin_session_count(database: &TestDatabase) -> i64 {
    database.query("SELECT count(*) FROM admin_sessions").await[0].get(0)
}

#[tokio::test]
async fn the_daily_cleanup_deletes_only_expired_web_and_admin_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(USER_NAME, USER_PASSWORD).await;
    sign_in(&server, "web", "Old laptop").await;
    sign_in(&server, "appleTv", "Living Room").await;
    sign_in(&server, "googleTv", "Bedroom").await;
    server
        .clock
        .advance(SignedDuration::from_hours(29 * 24 + 12));
    sign_in_admin(&server).await;
    server.clock.advance(SignedDuration::from_hours(1));
    sign_in(&server, "web", "Firefox on macOS").await;
    let mut admin = sign_in_admin(&server).await;
    server.clock.advance(SignedDuration::from_hours(11));

    application::delete_expired_sessions::call(
        application::delete_expired_sessions::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        device_names(&server, id).await,
        ["Living Room", "Bedroom", "Firefox on macOS"]
    );
    assert_eq!(admin_session_count(&database).await, 1);
    assert_eq!(admin.get("/admin/sync").await.status, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn starting_the_server_deletes_expired_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(USER_NAME, USER_PASSWORD).await;
    sign_in(&server, "web", "Old laptop").await;
    sign_in(&server, "appleTv", "Living Room").await;
    sign_in_admin(&server).await;
    let clock = server.clock.clone();
    drop(server);
    clock.advance(SignedDuration::from_hours(30 * 24));

    let server = TestServer::start_with(support::settings(database.url()), clock).await;

    assert_eq!(device_names(&server, id).await, ["Living Room"]);
    assert_eq!(admin_session_count(&database).await, 0);
    drop(server);
    database.drop().await;
}
