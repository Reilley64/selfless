use application::BuildInfo;
use application::ports::AcquireLock;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::ExportTelemetryError;
use application::ports::LockGuard;
use application::ports::SendEmail;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use infrastructure::AppState;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;

pub struct TestApp {
    pub state: AppState,
    pub clock: FakeClock,
    pub mailer: FakeMailer,
}

impl TestApp {
    pub async fn new() -> Self {
        let clock = FakeClock::new();
        let mailer = FakeMailer::default();
        let state = AppState {
            acquire_lock: AcquireLock::new(|_| Box::pin(async { LockGuard::new(()) })),
            build_info: BuildInfo {
                name: "selfless-test",
                version: "0.0.0-test",
                git_branch: "main",
                git_commit_id: "00000000",
                git_commit_time: "2026-01-01T00:00:00Z",
            },
            clock: Clock::new(clock.reader()),
            db_context: DbContext::new(
                test_fixtures::mock_database(persistence::models()).await,
                DbLocation {
                    database: "mock".to_owned(),
                    address: "localhost".to_owned(),
                    port: 0,
                },
            ),
            export_telemetry: ExportTelemetry::new(|_, _| {
                Box::pin(async { Err(ExportTelemetryError::default()) })
            }),
            send_email: SendEmail::new(mailer.sender()),
        };
        Self {
            state,
            clock,
            mailer,
        }
    }
}
