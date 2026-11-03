use application::BuildInfo;
use application::library::MediaFolder;
use application::library::Reconciles;
use application::library::SourceHealth;
use application::ports::AcquireLock;
use application::ports::CheckRadarrKodiMetadata;
use application::ports::CheckSonarrKodiMetadata;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::GetRadarrMovie;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::ListRadarrMovies;
use application::ports::ListSonarrShows;
use application::ports::ReadRadarrWebhook;
use application::ports::ReadSonarrWebhook;
use application::ports::SendEmail;
use application::ports::persistence::DbContext;
use application::webhooks::WebhookPasswords;
use jiff::Timestamp;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub acquire_lock: AcquireLock,
    pub build_info: BuildInfo,
    pub check_radarr_kodi_metadata: CheckRadarrKodiMetadata,
    pub check_sonarr_kodi_metadata: CheckSonarrKodiMetadata,
    pub clock: Clock,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
    pub get_radarr_movie: GetRadarrMovie,
    pub get_sonarr_episodes: GetSonarrEpisodes,
    pub get_sonarr_show: GetSonarrShow,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_shows: ListSonarrShows,
    pub media_folder: MediaFolder,
    pub read_radarr_webhook: ReadRadarrWebhook,
    pub read_sonarr_webhook: ReadSonarrWebhook,
    pub reconciles: Reconciles,
    pub send_email: SendEmail,
    pub source_health: SourceHealth,
    pub webhook_passwords: WebhookPasswords,
}

pub fn system_clock() -> Clock {
    Clock::new(Timestamp::now)
}
