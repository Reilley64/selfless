mod support;

use reqwest::StatusCode;
use sha2::Digest;
use sha2::Sha256;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::TestDatabase;
use support::TestServer;
use tokio::time::Instant;

#[tokio::test]
async fn the_database_stores_only_the_sha_256_hash_of_the_admin_session_token() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    browser.sign_in(ADMIN_NAME, ADMIN_PASSWORD).await;

    let token = browser.cookie("session").unwrap();
    let rows = database
        .query("SELECT token_hash FROM admin_sessions")
        .await;
    let stored: Vec<u8> = rows[0].get("token_hash");
    assert_eq!(rows.len(), 1);
    assert_eq!(stored, Sha256::digest(token.as_bytes()).to_vec());
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_name_and_a_wrong_password_return_the_same_error() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let wrong_name = browser.sign_in("Noor", ADMIN_PASSWORD).await;
    let wrong_password = browser.sign_in(ADMIN_NAME, "wrong").await;

    assert_eq!(wrong_name.status, StatusCode::OK);
    assert!(wrong_name.body.contains("Wrong name or password."));
    assert_eq!(wrong_name.status, wrong_password.status);
    assert_eq!(wrong_name.body, wrong_password.body);
    assert_eq!(browser.cookie("session"), None);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn admin_names_ignore_case_at_sign_in() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();

    let signed_in = browser
        .sign_in(&ADMIN_NAME.to_uppercase(), ADMIN_PASSWORD)
        .await;

    assert_eq!(signed_in.status, StatusCode::SEE_OTHER);
    assert_eq!(signed_in.location.as_deref(), Some("/admin"));
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn an_unknown_name_takes_as_long_as_a_wrong_password() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let mut browser = server.browser();
    let csrf = browser.get("/admin/sign-in").await.csrf_token();
    let mut sign_in_time = async |name: &str| {
        let start = Instant::now();
        browser
            .post_form(
                "/admin/sign-in",
                &[("csrf", &csrf), ("name", name), ("password", "wrong")],
            )
            .await;
        start.elapsed()
    };

    let wrong_password = sign_in_time(ADMIN_NAME).await;
    let unknown_name = sign_in_time("Noor").await;

    assert!(
        unknown_name * 2 > wrong_password,
        "unknown name {unknown_name:?}, wrong password {wrong_password:?}"
    );
    drop(server);
    database.drop().await;
}
