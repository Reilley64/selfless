use crate::Actor;
use crate::Artwork;
use crate::MediaFile;
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
