use application::ports::Clock;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::SendEmail;
use settings::AdminSettings;
use settings::AppSettings;
use settings::AssetSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::LibrarySyncSettings;
use settings::OtlpSettings;
use settings::RadarrSettings;
use settings::SmtpSettings;
use settings::SonarrSettings;
use settings::WebSettings;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestServer;
use tokio_util::sync::CancellationToken;
use url::Url;

pub const ADMIN_NAME: &str = "Maya";
pub const ADMIN_PASSWORD: &str = "correct horse battery staple";
pub const RADARR_WEBHOOK_PASSWORD: &str = "radarr webhook password";
pub const RADARR_API_KEY: &str = "radarr api key";
pub const SONARR_WEBHOOK_PASSWORD: &str = "sonarr webhook password";
pub const SONARR_API_KEY: &str = "sonarr api key";

pub fn settings(database_url: Url) -> AppSettings {
    AppSettings {
        http: HttpSettings { port: 0 },
        database: DatabaseSettings { url: database_url },
        web: WebSettings::default(),
        assets: AssetSettings {
            dir: Some(
                std::env::current_exe()
                    .unwrap()
                    .parent()
                    .and_then(|x| x.parent())
                    .unwrap()
                    .join("assets"),
            ),
        },
        admin: AdminSettings {
            username: Some(ADMIN_NAME.to_owned()),
            password: Some(ADMIN_PASSWORD.to_owned()),
        },
        otlp: OtlpSettings::default(),
        smtp: SmtpSettings::default(),
        radarr: RadarrSettings {
            url: Some("http://127.0.0.1:1".parse().unwrap()),
            api_key: Some(RADARR_API_KEY.to_owned()),
            webhook_password: Some(RADARR_WEBHOOK_PASSWORD.to_owned()),
        },
        sonarr: SonarrSettings {
            url: Some("http://127.0.0.1:1".parse().unwrap()),
            api_key: Some(SONARR_API_KEY.to_owned()),
            webhook_password: Some(SONARR_WEBHOOK_PASSWORD.to_owned()),
        },
        library_sync: LibrarySyncSettings::default(),
    }
}

pub async fn start(settings: AppSettings, clock: &FakeClock, mailer: &FakeMailer) -> TestServer {
    let server = selfless::start(
        settings,
        Clock::new(clock.reader()),
        SendEmail::new(mailer.sender()),
        ProbeMediaFile::new(|_| Box::pin(async { Err(ProbeError::default()) })),
    )
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
    TestServer::new(address, shutdown)
}
