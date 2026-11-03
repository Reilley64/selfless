use application::ports::Clock;
use application::ports::SendEmail;
use settings::AdminSettings;
use settings::AppSettings;
use settings::DatabaseSettings;
use settings::HttpSettings;
use settings::OtlpSettings;
use settings::SmtpSettings;
use settings::WebSettings;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestServer;
use tokio_util::sync::CancellationToken;
use url::Url;

pub const ADMIN_NAME: &str = "Maya";
pub const ADMIN_PASSWORD: &str = "correct horse battery staple";

pub fn settings(database_url: Url) -> AppSettings {
    AppSettings {
        http: HttpSettings { port: 0 },
        database: DatabaseSettings { url: database_url },
        web: WebSettings::default(),
        admin: AdminSettings {
            username: Some(ADMIN_NAME.to_owned()),
            password: Some(ADMIN_PASSWORD.to_owned()),
        },
        otlp: OtlpSettings::default(),
        smtp: SmtpSettings::default(),
    }
}

pub async fn start(settings: AppSettings, clock: &FakeClock, mailer: &FakeMailer) -> TestServer {
    let server = selfless::start(
        settings,
        Clock::new(clock.reader()),
        SendEmail::new(mailer.sender()),
    )
    .await
    .unwrap();
    let address = server.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    tokio::spawn(server.serve(shutdown.clone().cancelled_owned()));
    TestServer::new(address, shutdown)
}
