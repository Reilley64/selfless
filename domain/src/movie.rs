use crate::Actor;
use crate::Artwork;
use crate::MediaFile;
use crate::MovieMetadata;
use crate::SyncProblem;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Movie {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub tmdb_id: i64,
    #[index]
    pub radarr_id: i64,
    pub title: String,
    pub year: i64,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub added_at: Option<Timestamp>,
    #[index]
    pub media_file_id: Option<Uuid>,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<Option<MediaFile>>,
    pub visible: bool,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub actors: toasty::Deferred<Vec<Actor>>,
    #[has_many]
    pub artwork: toasty::Deferred<Vec<Artwork>>,
    #[has_many]
    pub sync_problems: toasty::Deferred<Vec<SyncProblem>>,
}

impl Movie {
    pub fn in_library(radarr_has_file: bool, file_on_disk: bool) -> bool {
        radarr_has_file && file_on_disk
    }

    pub fn in_library_filter() -> toasty::stmt::Expr<bool> {
        Self::fields().visible().eq(true)
    }

    pub fn metadata(&self) -> MovieMetadata {
        MovieMetadata {
            title: self.title.clone(),
            sort_title: self.sort_title.clone(),
            original_title: self.original_title.clone(),
            year: self.year,
            overview: self.overview.clone(),
            runtime_minutes: self.runtime_minutes,
            certification: self.certification.clone(),
            premiered: self.premiered,
            studio: self.studio.clone(),
            genres: self.genres.clone(),
            rating: self.rating,
            directors: self.directors.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_movie_is_in_the_library_when_radarr_has_its_file_and_the_file_is_on_disk() {
        assert!(Movie::in_library(true, true));
    }

    #[test]
    fn a_movie_radarr_has_no_file_for_is_not_in_the_library() {
        assert!(!Movie::in_library(false, false));
    }

    #[test]
    fn a_movie_whose_file_is_missing_on_disk_is_not_in_the_library() {
        assert!(!Movie::in_library(true, false));
    }
}
