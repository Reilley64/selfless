use crate::library::Prober;
use crate::library::SourceHealth;
use crate::ports::FindMediaFile;
use crate::ports::GetRadarrMovie;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub find_media_file: FindMediaFile,
    pub get_radarr_movie: GetRadarrMovie,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub source_health: SourceHealth,
}
