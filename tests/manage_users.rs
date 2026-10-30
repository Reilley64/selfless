mod support;

use domain::ClientKind;
use domain::Password;
use jiff::SignedDuration;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use serde_json::json;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;
use uuid::Uuid;

async fn me(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(Method::GET, "/api/me/v1", Some(token), None)
        .await
        .0
}

async fn sign_in(server: &TestServer, client_kind: &str, device_name: &str) -> String {
    let (status, body) = server
        .request(
            Method::POST,
            "/api/sessions/v1",
            None,
            Some(json!({
                "name": NAME,
                "password": PASSWORD,
                "clientKind": client_kind,
                "deviceName": device_name,
                "clientId": Uuid::new_v4(),
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    body["token"].as_str().unwrap().to_owned()
}

async fn list_sessions(server: &TestServer, user_id: Uuid) -> Vec<Value> {
    application::sessions::list_user_sessions::call(
        application::sessions::list_user_sessions::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
        },
        user_id,
    )
    .await
    .unwrap()
    .sessions
    .into_iter()
    .map(|x| {
        json!({
            "clientKind": match x.client_kind {
                ClientKind::Web => "web",
                ClientKind::AppleTv => "appleTv",
                ClientKind::GoogleTv => "googleTv",
            },
            "deviceName": x.device_name,
            "signedInAt": x.signed_in_at.to_string(),
            "lastUsedAt": x.last_used_at.to_string(),
        })
    })
    .collect()
}

#[tokio::test]
async fn listing_a_users_sessions_shows_each_client_kind_device_and_time() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    let start = server.clock.now();
    let web = sign_in(&server, "web", "Firefox on macOS").await;
    server.clock.advance(SignedDuration::from_hours(2));
    sign_in(&server, "appleTv", "Living Room").await;
    server.clock.advance(SignedDuration::from_hours(2));
    me(&server, &web).await;

    let sessions = list_sessions(&server, id).await;

    let hours = |x| (start + SignedDuration::from_hours(x)).to_string();
    assert_eq!(
        sessions,
        [
            json!({
                "clientKind": "web",
                "deviceName": "Firefox on macOS",
                "signedInAt": start.to_string(),
                "lastUsedAt": hours(4),
            }),
            json!({
                "clientKind": "appleTv",
                "deviceName": "Living Room",
                "signedInAt": hours(2),
                "lastUsedAt": hours(2),
            }),
        ]
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_list_leaves_out_web_sessions_past_30_days_without_use() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    sign_in(&server, "web", "Firefox on macOS").await;
    sign_in(&server, "googleTv", "Bedroom").await;

    server
        .clock
        .advance(SignedDuration::from_hours(30 * 24) - SignedDuration::from_secs(1));
    let before = list_sessions(&server, id).await;
    server.clock.advance(SignedDuration::from_secs(1));
    let after = list_sessions(&server, id).await;

    let devices = |sessions: Vec<Value>| {
        sessions
            .into_iter()
            .map(|x| x["deviceName"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(devices(before), ["Firefox on macOS", "Bedroom"]);
    assert_eq!(devices(after), ["Bedroom"]);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn deleting_a_user_ends_their_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    let web = sign_in(&server, "web", "Firefox on macOS").await;
    let tv = sign_in(&server, "appleTv", "Living Room").await;

    application::users::delete_user::call(
        application::users::delete_user::Dependencies {
            db_context: server.db_context().await,
        },
        id,
    )
    .await
    .unwrap();

    assert_eq!(list_sessions(&server, id).await, Vec::<Value>::new());
    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::UNAUTHORIZED);
    let (status, _) = server.sign_in(NAME, PASSWORD, "web").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn ending_one_session_signs_out_only_that_session() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    let web = sign_in(&server, "web", "Firefox on macOS").await;
    let tv = sign_in(&server, "appleTv", "Living Room").await;
    let web_session = application::sessions::list_user_sessions::call(
        application::sessions::list_user_sessions::Dependencies {
            db_context: server.db_context().await,
            clock: server.clock.port(),
        },
        id,
    )
    .await
    .unwrap()
    .sessions
    .into_iter()
    .find(|x| x.device_name == "Firefox on macOS")
    .unwrap();

    application::sessions::end_session::call(
        application::sessions::end_session::Dependencies {
            db_context: server.db_context().await,
        },
        web_session.id,
    )
    .await
    .unwrap();

    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::OK);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn ending_all_of_a_users_sessions_leaves_other_users_signed_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    server.create_user("Ravi", "another password").await;
    let web = sign_in(&server, "web", "Firefox on macOS").await;
    let tv = sign_in(&server, "googleTv", "Bedroom").await;
    let (_, other) = server.sign_in("Ravi", "another password", "web").await;

    application::sessions::end_user_sessions::call(
        application::sessions::end_user_sessions::Dependencies {
            db_context: server.db_context().await,
        },
        id,
    )
    .await
    .unwrap();

    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me(&server, other["token"].as_str().unwrap()).await,
        StatusCode::OK
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn resetting_a_users_password_ends_all_their_sessions() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let id = server.create_user(NAME, PASSWORD).await;
    server.create_user("Ravi", "another password").await;
    let web = sign_in(&server, "web", "Firefox on macOS").await;
    let tv = sign_in(&server, "googleTv", "Bedroom").await;
    let (_, other) = server.sign_in("Ravi", "another password", "web").await;

    application::users::reset_user_password::call(
        application::users::reset_user_password::Dependencies {
            db_context: server.db_context().await,
        },
        id,
        Password::new("a new passphrase".to_owned()).unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(me(&server, &web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &tv).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me(&server, other["token"].as_str().unwrap()).await,
        StatusCode::OK
    );
    let (status, _) = server.sign_in(NAME, PASSWORD, "web").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = server.sign_in(NAME, "a new passphrase", "web").await;
    assert_eq!(status, StatusCode::CREATED);
    drop(server);
    database.drop().await;
}
