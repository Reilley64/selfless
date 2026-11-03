#[path = "../test/selfless.rs"]
pub mod selfless_test;

use reqwest::StatusCode;
use serde_json::json;
use std::process::Command;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;

#[tokio::test]
async fn info_reports_the_build_and_git_fields() {
    let database = TestDatabase::create().await;
    let server = selfless_test::start(
        selfless_test::settings(database.url()),
        &FakeClock::new(),
        &FakeMailer::default(),
    )
    .await;

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
