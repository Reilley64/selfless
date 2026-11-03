use crate::Artwork;
use crate::MediaFile;
use crate::Season;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Episode {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub season_id: Uuid,
    #[belongs_to(key = season_id, references = id)]
    pub season: toasty::Deferred<Season>,
    #[unique]
    pub tvdb_id: i64,
    pub number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub added_at: Option<Timestamp>,
    #[index]
    pub media_file_id: Option<Uuid>,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<Option<MediaFile>>,
    pub visible: bool,
    pub rating: Option<f64>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub artwork: toasty::Deferred<Vec<Artwork>>,
}
