#[path = "../test/persistence.rs"]
mod persistence_test;
#[path = "../test/selfless.rs"]
pub mod selfless_test;

use application::ports::Clock;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use application::ports::persistence::DbScope;
use domain::PlayMethod;
use jiff::Timestamp;
use selfless_test::ADMIN_NAME;
use selfless_test::ADMIN_PASSWORD;
use selfless_test::RADARR_API_KEY;
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
use uuid::Uuid;

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

const RADARR_SETTINGS: [(&str, &str); 3] = [
    ("RADARR__URL", "http://127.0.0.1:7878"),
    ("RADARR__API_KEY", RADARR_API_KEY),
    ("RADARR__WEBHOOK_PASSWORD", RADARR_WEBHOOK_PASSWORD),
];
const SONARR_SETTINGS: [(&str, &str); 3] = [
    ("SONARR__URL", "http://127.0.0.1:8989"),
    ("SONARR__API_KEY", SONARR_API_KEY),
    ("SONARR__WEBHOOK_PASSWORD", SONARR_WEBHOOK_PASSWORD),
];

fn assert_exits_naming(missing: &str, settings: &[(&str, &str)]) {
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

#[test]
fn the_server_exits_naming_a_missing_radarr_setting() {
    for (missing, _) in RADARR_SETTINGS {
        assert_exits_naming(missing, &RADARR_SETTINGS);
    }
}

#[test]
fn the_server_exits_naming_a_missing_sonarr_setting() {
    let settings = [RADARR_SETTINGS, SONARR_SETTINGS].concat();
    for (missing, _) in SONARR_SETTINGS {
        assert_exits_naming(missing, &settings);
    }
}

#[tokio::test]
async fn the_server_refuses_to_start_naming_a_missing_image_cache_dir_segments_dir_or_signed_url_key()
 {
    let unreachable = || {
        settings(
            "postgres://selfless:selfless@127.0.0.1:1/selfless"
                .parse()
                .unwrap(),
        )
    };
    let mut without_cache_dir = unreachable();
    without_cache_dir.images.cache_dir = None;
    let mut without_segments_dir = unreachable();
    without_segments_dir.playback.segments_dir = None;
    let mut without_key = unreachable();
    without_key.signed_urls.key = None;
    let mut empty_key = unreachable();
    empty_key.signed_urls.key = Some(String::new());

    for (settings, missing) in [
        (without_cache_dir, "IMAGES__CACHE_DIR"),
        (without_segments_dir, "PLAYBACK__SEGMENTS_DIR"),
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

#[tokio::test]
async fn the_server_ends_every_open_playback_when_it_starts() {
    let database = TestDatabase::create().await;
    let mut db_context = persistence_test::db_context(&database).await;
    let playback_id = db_context
        .playbacks()
        .create()
        .user_id(Uuid::new_v4())
        .session_id(Uuid::new_v4())
        .media_file_id(Uuid::new_v4())
        .method(PlayMethod::Remux)
        .position(0.0)
        .duration(6981.0)
        .paused(false)
        .started_at(Timestamp::UNIX_EPOCH)
        .last_reported_at(Timestamp::UNIX_EPOCH)
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id;
    let clock = FakeClock::new();

    let server =
        selfless_test::start(settings(database.url()), &clock, &FakeMailer::default()).await;

    let ended_at = db_context
        .playbacks()
        .find(playback_id)
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .unwrap()
        .ended_at;
    assert_eq!(ended_at, Some(clock.now()));
    drop(server);
    drop(db_context);
    database.drop().await;
}
