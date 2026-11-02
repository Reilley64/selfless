mod support;

use reqwest::StatusCode;
use support::ADMIN_NAME;
use support::ADMIN_PASSWORD;
use support::TestDatabase;
use support::TestServer;

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
