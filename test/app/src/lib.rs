use application::BuildInfo;
use application::library::Prober;
use application::library::Reconciles;
use application::library::SourceHealth;
use application::ports::AcquireLock;
use application::ports::CheckRadarrKodiMetadata;
use application::ports::CheckSonarrKodiMetadata;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::ExportTelemetryError;
use application::ports::FindMediaFile;
use application::ports::GetRadarrMovie;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::ListRadarrMovies;
use application::ports::ListSonarrShows;
use application::ports::ListSubtitleFiles;
use application::ports::LockGuard;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::RadarrError;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadRadarrWebhook;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::ReadSonarrWebhook;
use application::ports::SendEmail;
use application::ports::SonarrError;
use application::ports::UnreadableNfo;
use application::ports::UnreadableWebhook;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::webhooks::WebhookPasswords;
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
            check_radarr_kodi_metadata: CheckRadarrKodiMetadata::new(|| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            check_sonarr_kodi_metadata: CheckSonarrKodiMetadata::new(|| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
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
            find_media_file: FindMediaFile::new(|_| Box::pin(async { None })),
            get_radarr_movie: GetRadarrMovie::new(|_| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            get_sonarr_episodes: GetSonarrEpisodes::new(|_| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            get_sonarr_show: GetSonarrShow::new(|_| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            list_radarr_movies: ListRadarrMovies::new(|| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            list_sonarr_shows: ListSonarrShows::new(|| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
            prober: Prober::new(ProbeMediaFile::new(|_| {
                Box::pin(async { Err(ProbeError::default()) })
            })),
            read_episode_folder: ReadEpisodeFolder::new(|_| Box::pin(async { Default::default() })),
            read_episode_nfo: ReadEpisodeNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default()) })
            }),
            read_movie_folder: ReadMovieFolder::new(|_| Box::pin(async { Default::default() })),
            read_movie_nfo: ReadMovieNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default()) })
            }),
            read_radarr_webhook: ReadRadarrWebhook::new(|_| Err(UnreadableWebhook::default())),
            read_show_folder: ReadShowFolder::new(|_, _| Box::pin(async { Default::default() })),
            read_show_nfo: ReadShowNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) })),
            read_sonarr_webhook: ReadSonarrWebhook::new(|_| Err(UnreadableWebhook::default())),
            reconciles: Reconciles::default(),
            send_email: SendEmail::new(mailer.sender()),
            source_health: SourceHealth::default(),
            webhook_passwords: WebhookPasswords::default(),
        };
        Self {
            state,
            clock,
            mailer,
        }
    }
}
