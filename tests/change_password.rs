mod support;

use reqwest::Method;
use reqwest::StatusCode;
use serde_json::Value;
use serde_json::json;
use support::TestDatabase;
use support::TestServer;
use support::USER_NAME as NAME;
use support::USER_PASSWORD as PASSWORD;

async fn sign_in(server: &TestServer, client_kind: &str) -> String {
    let (status, body) = server.sign_in(NAME, PASSWORD, client_kind).await;
    assert_eq!(status, StatusCode::CREATED);
    body["token"].as_str().unwrap().to_owned()
}

async fn change_password(
    server: &TestServer,
    token: &str,
    current_password: &str,
    new_password: &str,
) -> (StatusCode, Value) {
    server
        .request(
            Method::PUT,
            "/api/me/password/v1",
            Some(token),
            Some(json!({
                "currentPassword": current_password,
                "newPassword": new_password,
            })),
        )
        .await
}

async fn me(server: &TestServer, token: &str) -> StatusCode {
    server
        .request(Method::GET, "/api/me/v1", Some(token), None)
        .await
        .0
}

#[tokio::test]
async fn changing_the_password_ends_every_other_session_of_the_user() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    server.create_user("Ravi", "another password").await;
    let current = sign_in(&server, "web").await;
    let other_web = sign_in(&server, "web").await;
    let apple_tv = sign_in(&server, "appleTv").await;
    let google_tv = sign_in(&server, "googleTv").await;
    let (_, ravi) = server.sign_in("Ravi", "another password", "web").await;

    let (status, _) = change_password(&server, &current, PASSWORD, "a new passphrase").await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(me(&server, &current).await, StatusCode::OK);
    assert_eq!(me(&server, &other_web).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &apple_tv).await, StatusCode::UNAUTHORIZED);
    assert_eq!(me(&server, &google_tv).await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        me(&server, ravi["token"].as_str().unwrap()).await,
        StatusCode::OK
    );
    assert_eq!(
        server.sign_in(NAME, PASSWORD, "web").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server.sign_in(NAME, "a new passphrase", "web").await.0,
        StatusCode::CREATED
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_current_password_is_forbidden_and_changes_nothing() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let current = sign_in(&server, "web").await;
    let tv = sign_in(&server, "appleTv").await;

    let (status, body) =
        change_password(&server, &current, "not the password", "a new passphrase").await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["type"], "urn:selfless:problem:wrong-current-password");
    assert_eq!(me(&server, &tv).await, StatusCode::OK);
    assert_eq!(
        server.sign_in(NAME, PASSWORD, "web").await.0,
        StatusCode::CREATED
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_empty_new_password_is_rejected_and_changes_nothing() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    server.create_user(NAME, PASSWORD).await;
    let current = sign_in(&server, "web").await;
    let tv = sign_in(&server, "appleTv").await;

    let (status, body) = change_password(&server, &current, PASSWORD, "").await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["type"], "urn:selfless:problem:empty-password");
    assert_eq!(me(&server, &tv).await, StatusCode::OK);
    assert_eq!(
        server.sign_in(NAME, PASSWORD, "web").await.0,
        StatusCode::CREATED
    );
    drop(server);
    database.drop().await;
}
