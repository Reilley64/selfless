use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::civil::Date;
use toasty::Deferred;
use uuid::Uuid;

pub const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);

#[derive(Debug, toasty::Model)]
pub struct Admin {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    pub password_hash: String,
}

#[derive(Debug, toasty::Model)]
pub struct AdminSession {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub admin_id: Uuid,
    #[belongs_to]
    pub admin: Deferred<Admin>,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl AdminSession {
    pub const IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(12);
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl Colour {
    pub const ALL: [Self; 4] = [Self::Amber, Self::Blue, Self::Pink, Self::Emerald];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum Symbol {
    Ring,
    Triangle,
    Star,
    Waves,
}

impl Symbol {
    pub const ALL: [Self; 4] = [Self::Ring, Self::Triangle, Self::Star, Self::Waves];
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum ClientKind {
    Web,
    AppleTv,
    GoogleTv,
}

#[derive(Debug, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    #[unique]
    pub email: String,
    pub password_hash: String,
    pub colour: Colour,
    pub symbol: Symbol,
}

#[derive(Debug, toasty::Model)]
pub struct Session {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub user_id: Uuid,
    #[belongs_to]
    pub user: Deferred<User>,
    pub client_kind: ClientKind,
    pub device_name: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl Session {
    pub const WEB_IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(30 * 24);
}

#[derive(Debug, toasty::Model)]
pub struct Invite {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub admin_id: Uuid,
    pub sent_at: Timestamp,
    pub expires_at: Timestamp,
}

impl Invite {
    pub const VALID_FOR: SignedDuration = SignedDuration::from_hours(7 * 24);
}

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

impl ReconcileResult {
    pub fn name(self) -> &'static str {
        match self {
            ReconcileResult::Succeeded => "succeeded",
            ReconcileResult::Failed => "failed",
        }
    }
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
    pub media_file: Deferred<Option<MediaFile>>,
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
    pub actors: Deferred<Vec<Actor>>,
    #[has_many]
    pub artwork: Deferred<Vec<Artwork>>,
    #[has_many]
    pub sync_problems: Deferred<Vec<SyncProblem>>,
}

#[derive(Debug, toasty::Model)]
pub struct Actor {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: Deferred<Option<Movie>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: Deferred<Option<Show>>,
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
    Thumb,
}

#[derive(Debug, toasty::Model)]
pub struct Artwork {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: Deferred<Option<Movie>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: Deferred<Option<Show>>,
    #[index]
    pub season_id: Option<Uuid>,
    #[belongs_to(key = season_id, references = id)]
    pub season: Deferred<Option<Season>>,
    #[index]
    pub episode_id: Option<Uuid>,
    #[belongs_to(key = episode_id, references = id)]
    pub episode: Deferred<Option<Episode>>,
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum SyncProblemKind {
    UnreadableNfo,
    FileNotFound,
    FfprobeFailed,
}

#[derive(Debug, toasty::Model)]
pub struct SyncProblem {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub movie_id: Option<Uuid>,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: Deferred<Option<Movie>>,
    #[index]
    pub media_file_id: Option<Uuid>,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: Deferred<Option<MediaFile>>,
    #[index]
    pub show_id: Option<Uuid>,
    #[belongs_to(key = show_id, references = id)]
    pub show: Deferred<Option<Show>>,
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
    pub sort_title: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub rating: Option<f64>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub seasons: Deferred<Vec<Season>>,
    #[has_many]
    pub actors: Deferred<Vec<Actor>>,
    #[has_many]
    pub artwork: Deferred<Vec<Artwork>>,
    #[has_many]
    pub sync_problems: Deferred<Vec<SyncProblem>>,
}

#[derive(Debug, toasty::Model)]
pub struct Season {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub show_id: Uuid,
    #[belongs_to(key = show_id, references = id)]
    pub show: Deferred<Show>,
    pub number: i64,
    #[has_many]
    pub episodes: Deferred<Vec<Episode>>,
    #[has_many]
    pub artwork: Deferred<Vec<Artwork>>,
}

#[derive(Debug, toasty::Model)]
pub struct Episode {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub season_id: Uuid,
    #[belongs_to(key = season_id, references = id)]
    pub season: Deferred<Season>,
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
    pub media_file: Deferred<Option<MediaFile>>,
    pub visible: bool,
    pub rating: Option<f64>,
    pub nfo_modified_at: Option<Timestamp>,
    #[has_many]
    pub artwork: Deferred<Vec<Artwork>>,
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
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    #[has_many]
    pub movies: Deferred<Vec<Movie>>,
    #[has_many]
    pub episodes: Deferred<Vec<Episode>>,
    #[has_many]
    pub video_streams: Deferred<Vec<VideoStream>>,
    #[has_many]
    pub audio_streams: Deferred<Vec<AudioStream>>,
    #[has_many]
    pub subtitle_streams: Deferred<Vec<SubtitleStream>>,
    #[has_many]
    pub subtitle_files: Deferred<Vec<SubtitleFile>>,
    #[has_many]
    pub sync_problems: Deferred<Vec<SyncProblem>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum HdrType {
    Sdr,
    Hdr10,
    Hlg,
    DolbyVision,
}

#[derive(Debug, toasty::Model)]
pub struct VideoStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: i64,
    pub height: i64,
    pub frame_rate: Option<f64>,
    pub hdr_type: HdrType,
}

#[derive(Debug, toasty::Model)]
pub struct AudioStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub channels: i64,
    pub title: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, toasty::Model)]
pub struct SubtitleStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub text: bool,
}

#[derive(Debug, toasty::Model)]
pub struct SubtitleFile {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: Deferred<MediaFile>,
    pub path: String,
    pub language: Option<String>,
    pub hearing_impaired: bool,
    pub forced: bool,
}
