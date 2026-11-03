use crate::AudioStream;
use crate::Episode;
use crate::Movie;
use crate::SubtitleFile;
use crate::SubtitleStream;
use crate::SyncProblem;
use crate::VideoStream;
use jiff::Timestamp;
use uuid::Uuid;

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
    pub movies: toasty::Deferred<Vec<Movie>>,
    #[has_many]
    pub episodes: toasty::Deferred<Vec<Episode>>,
    #[has_many]
    pub video_streams: toasty::Deferred<Vec<VideoStream>>,
    #[has_many]
    pub audio_streams: toasty::Deferred<Vec<AudioStream>>,
    #[has_many]
    pub subtitle_streams: toasty::Deferred<Vec<SubtitleStream>>,
    #[has_many]
    pub subtitle_files: toasty::Deferred<Vec<SubtitleFile>>,
    #[has_many]
    pub sync_problems: toasty::Deferred<Vec<SyncProblem>>,
}
