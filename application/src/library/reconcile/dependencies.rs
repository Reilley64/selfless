use crate::library::Prober;
use crate::library::Reconciles;
use crate::library::SourceHealth;
use crate::ports::CheckRadarrKodiMetadata;
use crate::ports::CheckSonarrKodiMetadata;
use crate::ports::Clock;
use crate::ports::FindMediaFile;
use crate::ports::GetSonarrEpisodes;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrShows;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub check_radarr_kodi_metadata: CheckRadarrKodiMetadata,
    pub check_sonarr_kodi_metadata: CheckSonarrKodiMetadata,
    pub clock: Clock,
    pub db_context: DbContext,
    pub find_media_file: FindMediaFile,
    pub get_sonarr_episodes: GetSonarrEpisodes,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_shows: ListSonarrShows,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}
