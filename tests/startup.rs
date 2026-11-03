#[path = "../test/selfless.rs"]
mod selfless_test;

use application::ports::Clock;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use selfless_test::ADMIN_NAME;
use selfless_test::ADMIN_PASSWORD;
use selfless_test::RADARR_WEBHOOK_PASSWORD;
use selfless_test::SONARR_API_KEY;
use selfless_test::SONARR_WEBHOOK_PASSWORD;
use selfless_test::settings;
use settings::AdminSettings;
use settings::AppSettings;
use std::process::Command;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;

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
            ProbeMediaFile::new(|_| Box::pin(async { Err(ProbeError::default()) })),
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

#[test]
fn the_server_exits_naming_a_missing_radarr_setting() {
    let settings = [
        ("RADARR__URL", "http://127.0.0.1:7878"),
        ("RADARR__API_KEY", "radarr api key"),
        ("RADARR__WEBHOOK_PASSWORD", RADARR_WEBHOOK_PASSWORD),
    ];
    for (missing, _) in settings {
        let output = Command::new(env!("CARGO_BIN_EXE_selfless"))
            .env(
                "DATABASE__URL",
                "postgres://selfless:selfless@127.0.0.1:1/selfless",
            )
            .envs(
                settings
                    .iter()
                    .filter(|(name, _)| *name != missing)
                    .copied(),
            )
            .env_remove(missing)
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(missing), "{missing} is not named: {stderr}");
    }
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

#[tokio::test]
async fn the_server_refuses_to_start_naming_a_missing_image_cache_dir_or_signed_url_key() {
    let unreachable = || {
        settings(
            "postgres://selfless:selfless@127.0.0.1:1/selfless"
                .parse()
                .unwrap(),
        )
    };
    let mut without_cache_dir = unreachable();
    without_cache_dir.images.cache_dir = None;
    let mut without_key = unreachable();
    without_key.signed_urls.key = None;
    let mut empty_key = unreachable();
    empty_key.signed_urls.key = Some(String::new());

    for (settings, missing) in [
        (without_cache_dir, "IMAGES__CACHE_DIR"),
        (without_key, "SIGNED_URLS__KEY"),
        (empty_key, "SIGNED_URLS__KEY"),
    ] {
        let message = start_error(settings).await;

        assert!(message.contains(missing), "{message}");
    }
}

async fn start_error(settings: AppSettings) -> String {
    selfless::start(
        settings,
        Clock::new(FakeClock::new().reader()),
        SendEmail::new(FakeMailer::default().sender()),
        ProbeMediaFile::new(|_| Box::pin(async { Err(ProbeError::default()) })),
    )
    .await
    .err()
    .expect("the server refuses to start")
    .to_string()
}
