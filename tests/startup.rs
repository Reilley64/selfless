#[path = "../test/selfless.rs"]
mod selfless_test;

use application::ports::Clock;
use application::ports::SendEmail;
use selfless_test::ADMIN_NAME;
use selfless_test::ADMIN_PASSWORD;
use selfless_test::settings;
use settings::AdminSettings;
use std::process::Command;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;

#[test]
fn the_server_exits_naming_each_missing_smtp_setting() {
    let output = Command::new(env!("CARGO_BIN_EXE_selfless"))
        .env(
            "DATABASE__URL",
            "postgres://selfless:selfless@127.0.0.1:1/selfless",
        )
        .env("SMTP__HOST", "smtp.example.com")
        .env("SMTP__PORT", "587")
        .env_remove("SMTP__USERNAME")
        .env_remove("SMTP__PASSWORD")
        .env_remove("SMTP__FROM")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    for missing in ["SMTP__USERNAME", "SMTP__PASSWORD", "SMTP__FROM"] {
        assert!(stderr.contains(missing), "{missing} is not named: {stderr}");
    }
    for present in ["SMTP__HOST", "SMTP__PORT"] {
        assert!(!stderr.contains(present), "{present} is named: {stderr}");
    }
}

#[tokio::test]
async fn the_server_refuses_to_start_without_a_first_admin() {
    let database = TestDatabase::create().await;

    for (username, password) in [
        (None, None),
        (Some(ADMIN_NAME), None),
        (None, Some(ADMIN_PASSWORD)),
        (Some(""), Some(ADMIN_PASSWORD)),
        (Some(ADMIN_NAME), Some("")),
    ] {
        let mut settings = settings(database.url());
        settings.admin = AdminSettings {
            username: username.map(str::to_owned),
            password: password.map(str::to_owned),
        };

        let error = selfless::start(
            settings,
            Clock::new(FakeClock::new().reader()),
            SendEmail::new(FakeMailer::default().sender()),
        )
        .await
        .err()
        .expect("the server refuses to start");

        let message = error.to_string();
        assert!(message.contains("ADMIN__USERNAME"), "{message}");
        assert!(message.contains("ADMIN__PASSWORD"), "{message}");
    }
    database.drop().await;
}

#[tokio::test]
async fn once_an_admin_exists_the_server_starts_with_the_first_admin_settings_empty() {
    let database = TestDatabase::create().await;
    let clock = FakeClock::new();
    let mailer = FakeMailer::default();
    drop(selfless_test::start(settings(database.url()), &clock, &mailer).await);
    let mut settings = settings(database.url());
    settings.admin = AdminSettings {
        username: Some(String::new()),
        password: Some(String::new()),
    };

    drop(selfless_test::start(settings, &clock, &mailer).await);

    database.drop().await;
}
