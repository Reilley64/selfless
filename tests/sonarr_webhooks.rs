mod support;

use reqwest::Method;
use reqwest::StatusCode;
use std::process::Command;
use support::RADARR_WEBHOOK_PASSWORD;
use support::SONARR_API_KEY;
use support::SONARR_WEBHOOK_PASSWORD;
use support::TestDatabase;
use support::TestServer;
use support::fixture;

#[tokio::test]
async fn an_accepted_test_webhook_is_the_last_sonarr_webhook_in_the_sync_status() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let status = server
        .send_sonarr_webhook(
            Method::POST,
            Some(("sonarr", SONARR_WEBHOOK_PASSWORD)),
            &fixture("sonarr/webhooks/test.json"),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    let sync_status = application::sync_status::call(&server.state).await.unwrap();
    assert_eq!(sync_status.sonarr.last_webhook_at, Some(server.clock.now()));
    assert_eq!(sync_status.sonarr.rejected_webhooks, 0);
    assert_eq!(sync_status.radarr.last_webhook_at, None);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn a_wrong_or_missing_sonarr_password_is_rejected_and_counted_in_the_sync_status() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;
    let body = fixture("sonarr/webhooks/test.json");

    let wrong = server
        .send_sonarr_webhook(Method::PUT, Some(("sonarr", "wrong password")), &body)
        .await;
    let missing = server.send_sonarr_webhook(Method::POST, None, &body).await;

    assert_eq!(wrong, StatusCode::UNAUTHORIZED);
    assert_eq!(missing, StatusCode::UNAUTHORIZED);
    let sync_status = application::sync_status::call(&server.state).await.unwrap();
    assert_eq!(sync_status.sonarr.rejected_webhooks, 2);
    assert_eq!(sync_status.sonarr.last_webhook_at, None);
    assert_eq!(sync_status.radarr.rejected_webhooks, 0);
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn the_radarr_password_does_not_open_the_sonarr_endpoint() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let status = server
        .send_sonarr_webhook(
            Method::POST,
            Some(("sonarr", RADARR_WEBHOOK_PASSWORD)),
            &fixture("sonarr/webhooks/test.json"),
        )
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    drop(server);
    database.drop().await;
}

#[test]
fn the_server_exits_naming_a_missing_sonarr_setting() {
    let settings = [
        ("RADARR__URL", "http://127.0.0.1:7878"),
        ("RADARR__API_KEY", "radarr api key"),
        ("RADARR__WEBHOOK_PASSWORD", RADARR_WEBHOOK_PASSWORD),
        ("SONARR__URL", "http://127.0.0.1:8989"),
        ("SONARR__API_KEY", SONARR_API_KEY),
        ("SONARR__WEBHOOK_PASSWORD", SONARR_WEBHOOK_PASSWORD),
    ];
    for (missing, _) in settings
        .iter()
        .filter(|(name, _)| name.starts_with("SONARR"))
    {
        let output = Command::new(env!("CARGO_BIN_EXE_selfless"))
            .env(
                "DATABASE__URL",
                "postgres://selfless:selfless@127.0.0.1:1/selfless",
            )
            .envs(settings.iter().filter(|(name, _)| name != missing).copied())
            .env_remove(missing)
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(missing), "{missing} is not named: {stderr}");
    }
}
