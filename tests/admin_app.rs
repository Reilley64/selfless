mod support;

use reqwest::StatusCode;
use support::TestDatabase;
use support::TestServer;

#[tokio::test]
async fn admin_shows_the_name_and_version() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (status, body) = server.get_text("/admin").await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains(">selfless<"), "{body}");
    assert!(
        body.contains(&format!(">{}<", env!("CARGO_PKG_VERSION"))),
        "{body}"
    );
    drop(server);
    database.drop().await;
}
