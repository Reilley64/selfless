mod support;

use reqwest::StatusCode;
use reqwest::header::CONTENT_SECURITY_POLICY;
use std::collections::HashMap;
use std::path::PathBuf;
use support::FakeClock;
use support::TestDatabase;
use support::TestServer;
use uuid::Uuid;

fn directives(policy: &str) -> HashMap<&str, Vec<&str>> {
    policy
        .split(';')
        .filter_map(|directive| {
            let mut parts = directive.split_whitespace();
            Some((parts.next()?, parts.collect()))
        })
        .collect()
}

async fn start_with_web_app(database: &TestDatabase) -> (TestServer, PathBuf) {
    let web_dir = std::env::temp_dir().join(format!("selfless-web-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&web_dir).unwrap();
    std::fs::write(web_dir.join("index.html"), "<title>selfless</title>").unwrap();
    let mut settings = support::settings(database.url());
    settings.web.dir = web_dir.clone();
    (
        TestServer::start_with(settings, FakeClock::new()).await,
        web_dir,
    )
}

#[tokio::test]
async fn the_web_app_comes_with_a_strict_content_security_policy() {
    let database = TestDatabase::create().await;
    let (server, web_dir) = start_with_web_app(&database).await;

    let (status, headers, html) = server.get_text("/").await;

    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<title>selfless</title>"), "{html}");
    let policy = headers[CONTENT_SECURITY_POLICY].to_str().unwrap();
    let directives = directives(policy);
    assert_eq!(directives["default-src"], ["'self'"], "{policy}");
    assert_eq!(directives["script-src"], ["'self'"], "{policy}");
    assert_eq!(directives["object-src"], ["'none'"], "{policy}");
    assert_eq!(directives["base-uri"], ["'none'"], "{policy}");
    assert_eq!(directives["frame-ancestors"], ["'none'"], "{policy}");
    assert!(!policy.contains("'unsafe-"), "{policy}");
    drop(server);
    std::fs::remove_dir_all(web_dir).unwrap();
    database.drop().await;
}

#[tokio::test]
async fn the_invite_screen_loads_from_the_invite_link() {
    let database = TestDatabase::create().await;
    let (server, web_dir) = start_with_web_app(&database).await;

    let (status, headers, html) = server.get_text("/invite").await;

    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<title>selfless</title>"), "{html}");
    assert!(headers.contains_key(CONTENT_SECURITY_POLICY));
    drop(server);
    std::fs::remove_dir_all(web_dir).unwrap();
    database.drop().await;
}

#[tokio::test]
async fn the_settings_screen_loads_from_its_own_address() {
    let database = TestDatabase::create().await;
    let (server, web_dir) = start_with_web_app(&database).await;

    let (status, headers, html) = server.get_text("/settings").await;

    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("<title>selfless</title>"), "{html}");
    assert!(headers.contains_key(CONTENT_SECURITY_POLICY));
    drop(server);
    std::fs::remove_dir_all(web_dir).unwrap();
    database.drop().await;
}
