use application::BuildInfo;
use application::DbContext;
use application::InboxWakeup;
use application::Reconciles;
use application::SourceHealth;
use application::WebhookPasswords;
use application::ports::CheckRadarrKodiMetadata;
use application::ports::CheckSonarrKodiMetadata;
use application::ports::Clock;
use application::ports::ExportTelemetry;
use application::ports::GetRadarrMovie;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::ListRadarrMovies;
use application::ports::ListSonarrSeries;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use jiff::Timestamp;

pub use media_files;
pub use persistence;
pub use radarr;
pub use sonarr;
pub use telemetry;

#[fundle::bundle]
#[derive(Clone)]
pub struct AppState {
    pub build_info: BuildInfo,
    pub check_radarr_kodi_metadata: CheckRadarrKodiMetadata,
    pub check_sonarr_kodi_metadata: CheckSonarrKodiMetadata,
    pub clock: Clock,
    pub db_context: DbContext,
    pub export_telemetry: ExportTelemetry,
    pub get_radarr_movie: GetRadarrMovie,
    pub get_sonarr_episodes: GetSonarrEpisodes,
    pub get_sonarr_show: GetSonarrShow,
    pub inbox_wakeup: InboxWakeup,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_series: ListSonarrSeries,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
    pub webhook_passwords: WebhookPasswords,
}

pub fn system_clock() -> Clock {
    Clock::new(Timestamp::now)
}
