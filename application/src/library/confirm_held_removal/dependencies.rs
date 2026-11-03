use crate::library::Reconciles;
use crate::library::SourceHealth;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrShows;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_shows: ListSonarrShows,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}
