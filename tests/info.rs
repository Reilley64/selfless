mod support;

use reqwest::StatusCode;
use serde_json::json;
use std::process::Command;
use support::TestDatabase;
use support::TestServer;

#[tokio::test]
async fn info_reports_the_build_and_git_fields() {
    let database = TestDatabase::create().await;
    let server = TestServer::start(database.url()).await;

    let (status, body) = server.get("/actuator/info/v1").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "build": {
                "name": "selfless",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "git": {
                "branch": git(&["rev-parse", "--abbrev-ref", "HEAD"]),
                "commit": {
                    "id": git(&["rev-parse", "HEAD"])[..8],
                    "time": git(&["show", "--no-patch", "--format=%cd", "--date=format-local:%Y-%m-%dT%H:%M:%SZ", "HEAD"]),
                },
            },
        })
    );
    drop(server);
    database.drop().await;
}

fn git(args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
