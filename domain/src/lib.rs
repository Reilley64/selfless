use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum Source {
    Radarr,
    Sonarr,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Radarr => "radarr",
            Source::Sonarr => "sonarr",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum InboxRowStatus {
    Pending,
    Processing,
    Processed,
    Ignored,
    Failed,
    Resolved,
}

#[derive(Debug, toasty::Model)]
pub struct InboxRow {
    #[key]
    #[auto]
    pub id: Uuid,
    pub source: Source,
    pub event_type: Option<String>,
    pub body: Vec<u8>,
    pub received_at: Timestamp,
    pub traceparent: Option<String>,
    #[index]
    pub status: InboxRowStatus,
    pub attempts: i64,
    pub next_attempt_at: Timestamp,
    pub error: Option<String>,
    pub source_item_id: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum ReconcileResult {
    Succeeded,
    Failed,
}

#[derive(Debug, toasty::Model)]
pub struct SourceState {
    #[key]
    pub source: Source,
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
    pub last_reconcile_started_at: Option<Timestamp>,
    pub last_reconcile_duration_ms: Option<i64>,
    pub last_reconcile_result: Option<ReconcileResult>,
}

#[derive(Debug, toasty::Model)]
pub struct HeldRemoval {
    #[key]
    pub source: Source,
    pub item_ids: Vec<Uuid>,
}

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
}

#[derive(Debug, toasty::Model)]
pub struct Actor {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Uuid,
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum ArtworkKind {
    Poster,
    Fanart,
    Clearlogo,
}

#[derive(Debug, toasty::Model)]
pub struct Artwork {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Uuid,
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum SyncProblemKind {
    UnreadableNfo,
    FileNotFound,
}

#[derive(Debug, toasty::Model)]
pub struct SyncProblem {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[index]
    pub media_file_id: Option<Uuid>,
    pub kind: SyncProblemKind,
    pub path: String,
}

#[derive(Debug, toasty::Model)]
pub struct Show {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub tvdb_id: i64,
    #[unique]
    pub sonarr_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
    #[has_many]
    pub seasons: toasty::Deferred<Vec<Season>>,
}

#[derive(Debug, toasty::Model)]
pub struct Season {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub show_id: Uuid,
    #[belongs_to(key = show_id, references = id)]
    pub show: toasty::Deferred<Show>,
    pub number: i64,
    #[has_many]
    pub episodes: toasty::Deferred<Vec<Episode>>,
}

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
}

#[derive(Debug, toasty::Model)]
pub struct MediaFile {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub path: String,
    pub size: Option<i64>,
    pub modified_at: Option<Timestamp>,
    #[has_many]
    pub movies: toasty::Deferred<Vec<Movie>>,
    #[has_many]
    pub episodes: toasty::Deferred<Vec<Episode>>,
}
