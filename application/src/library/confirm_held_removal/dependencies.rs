use crate::library::SourceHealth;
use crate::ports::AcquireLock;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrShows;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub acquire_lock: AcquireLock,
    pub db_context: DbContext,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_shows: ListSonarrShows,
    pub source_health: SourceHealth,
}
