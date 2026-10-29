mod support;

use reqwest::StatusCode;
use serde_json::json;
use support::Proxy;
use support::TestDatabase;
use support::TestServer;

#[tokio::test]
async fn health_is_up_when_postgresql_is_reachable() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (status, body) = server.get("/actuator/health/v1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "status": "UP",
            "components": {
                "db": { "status": "UP" },
                "livenessState": { "status": "UP" },
                "readinessState": { "status": "UP" },
            },
        })
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn liveness_is_up_when_postgresql_is_reachable() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (status, body) = server.get("/actuator/health/liveness/v1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "status": "UP",
            "components": {
                "livenessState": { "status": "UP" },
            },
        })
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn readiness_is_up_when_postgresql_is_reachable() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (status, body) = server.get("/actuator/health/readiness/v1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "status": "UP",
            "components": {
                "db": { "status": "UP" },
                "readinessState": { "status": "UP" },
            },
        })
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn readiness_is_down_and_liveness_stays_up_when_postgresql_is_unreachable() {
    let database = TestDatabase::create().await;
    let proxy = Proxy::start(&database.url()).await;
    let server = TestServer::start(proxy.url(&database.url())).await;

    proxy.stop();
    let (readiness_status, readiness) = server.get("/actuator/health/readiness/v1").await;
    let (liveness_status, liveness) = server.get("/actuator/health/liveness/v1").await;

    assert_eq!(readiness_status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        readiness,
        json!({
            "status": "DOWN",
            "components": {
                "db": { "status": "DOWN" },
                "readinessState": { "status": "UP" },
            },
        })
    );
    assert_eq!(liveness_status, StatusCode::OK);
    assert_eq!(
        liveness,
        json!({
            "status": "UP",
            "components": {
                "livenessState": { "status": "UP" },
            },
        })
    );
    drop(server);
    database.drop().await;
}

#[tokio::test]
async fn readiness_is_out_of_service_after_shutdown_starts() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    server.shut_down();
    let (status, body) = server.get("/actuator/health/readiness/v1").await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body,
        json!({
            "status": "OUT_OF_SERVICE",
            "components": {
                "db": { "status": "UP" },
                "readinessState": { "status": "OUT_OF_SERVICE" },
            },
        })
    );
    drop(server);
    database.drop().await;
}
