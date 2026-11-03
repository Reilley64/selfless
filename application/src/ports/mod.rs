use bytes::Bytes;
use domain::AudioTranscode;
use domain::EmailAddress;
use domain::EpisodeNfo;
use domain::HdrType;
use domain::MovieNfo;
use domain::RadarrId;
use domain::ShowNfo;
use domain::SonarrId;
use domain::TmdbId;
use domain::TvdbId;
use domain::VideoTranscode;
use jiff::Timestamp;
use jiff::civil::Date;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

pub mod persistence;

pub type PortFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

type AcquireLockFn = dyn Fn(String) -> PortFuture<LockGuard> + Send + Sync;

#[derive(Clone)]
pub struct AcquireLock(Arc<AcquireLockFn>);

impl AcquireLock {
    pub fn new(f: impl Fn(String) -> PortFuture<LockGuard> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, key: String) -> PortFuture<LockGuard> {
        (self.0)(key)
    }

    pub fn key(
        domain: &str,
        operation: &str,
        version: u8,
        parts: impl IntoIterator<Item = String>,
    ) -> String {
        let mut key = String::new();
        push_key_part(&mut key, domain);
        key.push(':');
        push_key_part(&mut key, operation);
        key.push_str(&format!(":v{version}"));
        for part in parts {
            key.push(':');
            push_key_part(&mut key, &part);
        }
        key
    }
}

fn push_key_part(key: &mut String, part: &str) {
    for character in part.chars() {
        match character {
            '%' => key.push_str("%25"),
            ':' => key.push_str("%3A"),
            _ => key.push(character),
        }
    }
}

#[must_use = "the lock is held only while this guard is alive"]
pub struct LockGuard {
    _held: Box<dyn Send>,
}

impl LockGuard {
    pub fn new(held: impl Send + 'static) -> Self {
        Self {
            _held: Box::new(held),
        }
    }
}

type CheckRadarrKodiMetadataFn = dyn Fn() -> PortFuture<Result<bool, RadarrError>> + Send + Sync;

#[derive(Clone)]
pub struct CheckRadarrKodiMetadata(Arc<CheckRadarrKodiMetadataFn>);

impl CheckRadarrKodiMetadata {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<bool, RadarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<bool, RadarrError>> {
        (self.0)()
    }
}

type CheckSonarrKodiMetadataFn = dyn Fn() -> PortFuture<Result<bool, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct CheckSonarrKodiMetadata(Arc<CheckSonarrKodiMetadataFn>);

impl CheckSonarrKodiMetadata {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<bool, SonarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<bool, SonarrError>> {
        (self.0)()
    }
}

type ClockFn = dyn Fn() -> Timestamp + Send + Sync;

#[derive(Clone)]
pub struct Clock(Arc<ClockFn>);

impl Clock {
    pub fn new(f: impl Fn() -> Timestamp + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> Timestamp {
        (self.0)()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CachedImage {
    pub image_id: Uuid,
    pub width: i64,
    pub modified_at: Timestamp,
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to use the image cache")]
pub struct ImageCacheError {
    core: ohno::OhnoCore,
}

impl ImageCacheError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type DeleteCachedImageFn =
    dyn Fn(CachedImage) -> PortFuture<Result<(), ImageCacheError>> + Send + Sync;

#[derive(Clone)]
pub struct DeleteCachedImage(Arc<DeleteCachedImageFn>);

impl DeleteCachedImage {
    pub fn new(
        f: impl Fn(CachedImage) -> PortFuture<Result<(), ImageCacheError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, image: &CachedImage) -> PortFuture<Result<(), ImageCacheError>> {
        (self.0)(image.clone())
    }
}

#[derive(Clone, Debug)]
pub struct HlsOutput {
    pub playback_id: Uuid,
    pub path: String,
    pub duration: Duration,
    pub keyframes: Vec<Duration>,
    pub video_transcode: Option<VideoTranscode>,
    pub video: VideoStream,
    pub audio: Vec<HlsAudioTrack>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub subtitle_files: Vec<FoundSubtitleFile>,
    pub selected_subtitle: Option<usize>,
    pub start_position: Duration,
}

#[derive(Clone, Debug)]
pub struct HlsAudioTrack {
    pub stream: AudioStream,
    pub transcode: Option<AudioTranscode>,
    pub selected: bool,
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to run an HLS output")]
pub struct HlsError {
    core: ohno::OhnoCore,
}

impl HlsError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type EndHlsOutputFn = dyn Fn(Uuid) -> PortFuture<()> + Send + Sync;

#[derive(Clone)]
pub struct EndHlsOutput(Arc<EndHlsOutputFn>);

impl EndHlsOutput {
    pub fn new(f: impl Fn(Uuid) -> PortFuture<()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, playback_id: Uuid) -> PortFuture<()> {
        (self.0)(playback_id)
    }
}

type ExportTelemetryFn =
    dyn Fn(TelemetrySignal, Bytes) -> PortFuture<Result<Bytes, ExportTelemetryError>> + Send + Sync;

#[derive(Clone)]
pub struct ExportTelemetry(Arc<ExportTelemetryFn>);

impl ExportTelemetry {
    pub fn new(
        f: impl Fn(TelemetrySignal, Bytes) -> PortFuture<Result<Bytes, ExportTelemetryError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        signal: TelemetrySignal,
        request: Bytes,
    ) -> PortFuture<Result<Bytes, ExportTelemetryError>> {
        (self.0)(signal, request)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to export telemetry")]
pub struct ExportTelemetryError {
    core: ohno::OhnoCore,
}

impl ExportTelemetryError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(Debug)]
pub struct MalformedTelemetry;

impl fmt::Display for MalformedTelemetry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the telemetry request is not a valid OTLP message")
    }
}

impl Error for MalformedTelemetry {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySignal {
    Traces,
    Metrics,
    Logs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoundMediaFile {
    pub size: i64,
    pub modified_at: Timestamp,
}

type FindMediaFileFn =
    dyn Fn(String) -> PortFuture<Result<Option<FoundMediaFile>, UnreadableFile>> + Send + Sync;

#[derive(Clone)]
pub struct FindMediaFile(Arc<FindMediaFileFn>);

impl FindMediaFile {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Option<FoundMediaFile>, UnreadableFile>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<Option<FoundMediaFile>, UnreadableFile>> {
        (self.0)(path.to_owned())
    }
}

type GetHlsFileFn =
    dyn Fn(Uuid, String) -> PortFuture<Result<Option<String>, HlsError>> + Send + Sync;

#[derive(Clone)]
pub struct GetHlsFile(Arc<GetHlsFileFn>);

impl GetHlsFile {
    pub fn new(
        f: impl Fn(Uuid, String) -> PortFuture<Result<Option<String>, HlsError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        playback_id: Uuid,
        name: &str,
    ) -> PortFuture<Result<Option<String>, HlsError>> {
        (self.0)(playback_id, name.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct SourceMovie {
    pub radarr_id: RadarrId,
    pub tmdb_id: TmdbId,
    pub title: String,
    pub year: i64,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub media_file: Option<SourceMediaFile>,
}

#[derive(Clone, Debug)]
pub struct SourceMediaFile {
    pub path: String,
    pub added_at: Timestamp,
}

type GetRadarrMovieFn =
    dyn Fn(RadarrId) -> PortFuture<Result<Option<SourceMovie>, RadarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetRadarrMovie(Arc<GetRadarrMovieFn>);

impl GetRadarrMovie {
    pub fn new(
        f: impl Fn(RadarrId) -> PortFuture<Result<Option<SourceMovie>, RadarrError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        radarr_id: RadarrId,
    ) -> PortFuture<Result<Option<SourceMovie>, RadarrError>> {
        (self.0)(radarr_id)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to call the Radarr API")]
pub struct RadarrError {
    core: ohno::OhnoCore,
}

impl RadarrError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type GetSonarrEpisodesFn =
    dyn Fn(SonarrId) -> PortFuture<Result<Vec<SourceEpisode>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetSonarrEpisodes(Arc<GetSonarrEpisodesFn>);

impl GetSonarrEpisodes {
    pub fn new(
        f: impl Fn(SonarrId) -> PortFuture<Result<Vec<SourceEpisode>, SonarrError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, sonarr_id: SonarrId) -> PortFuture<Result<Vec<SourceEpisode>, SonarrError>> {
        (self.0)(sonarr_id)
    }
}

#[derive(Clone)]
pub struct SourceShowListing {
    pub sonarr_id: SonarrId,
    pub tvdb_id: TvdbId,
    pub path: String,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
}

#[derive(Clone)]
pub struct SourceShow {
    pub listing: SourceShowListing,
    pub episodes: Vec<SourceEpisode>,
}

#[derive(Clone)]
pub struct SourceEpisode {
    pub sonarr_id: SonarrId,
    pub tvdb_id: TvdbId,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub media_file: Option<SourceMediaFile>,
}

type GetSonarrShowFn =
    dyn Fn(SonarrId) -> PortFuture<Result<Option<SourceShow>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetSonarrShow(Arc<GetSonarrShowFn>);

impl GetSonarrShow {
    pub fn new(
        f: impl Fn(SonarrId) -> PortFuture<Result<Option<SourceShow>, SonarrError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, sonarr_id: SonarrId) -> PortFuture<Result<Option<SourceShow>, SonarrError>> {
        (self.0)(sonarr_id)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to call the Sonarr API")]
pub struct SonarrError {
    core: ohno::OhnoCore,
}

impl SonarrError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type ListCachedImagesFn =
    dyn Fn() -> PortFuture<Result<Vec<CachedImage>, ImageCacheError>> + Send + Sync;

#[derive(Clone)]
pub struct ListCachedImages(Arc<ListCachedImagesFn>);

impl ListCachedImages {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<Vec<CachedImage>, ImageCacheError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<Vec<CachedImage>, ImageCacheError>> {
        (self.0)()
    }
}

type ListRadarrMoviesFn =
    dyn Fn() -> PortFuture<Result<Vec<SourceMovie>, RadarrError>> + Send + Sync;

#[derive(Clone)]
pub struct ListRadarrMovies(Arc<ListRadarrMoviesFn>);

impl ListRadarrMovies {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<Vec<SourceMovie>, RadarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<Vec<SourceMovie>, RadarrError>> {
        (self.0)()
    }
}

type ListSonarrShowsFn =
    dyn Fn() -> PortFuture<Result<Vec<SourceShowListing>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct ListSonarrShows(Arc<ListSonarrShowsFn>);

impl ListSonarrShows {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<Vec<SourceShowListing>, SonarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<Vec<SourceShowListing>, SonarrError>> {
        (self.0)()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FoundSubtitleFile {
    pub path: String,
    pub language: Option<String>,
    pub hearing_impaired: bool,
    pub forced: bool,
}

type ListSubtitleFilesFn = dyn Fn(String) -> PortFuture<Result<Vec<FoundSubtitleFile>, ListSubtitleFilesError>>
    + Send
    + Sync;

#[derive(Clone)]
pub struct ListSubtitleFiles(Arc<ListSubtitleFilesFn>);

impl ListSubtitleFiles {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Vec<FoundSubtitleFile>, ListSubtitleFilesError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        media_file_path: &str,
    ) -> PortFuture<Result<Vec<FoundSubtitleFile>, ListSubtitleFilesError>> {
        (self.0)(media_file_path.to_owned())
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to list the subtitle files beside a media file")]
pub struct ListSubtitleFilesError {
    core: ohno::OhnoCore,
}

impl ListSubtitleFilesError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Probe {
    pub container: String,
    pub duration: Option<Duration>,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoStream {
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: i64,
    pub height: i64,
    pub frame_rate: Option<f64>,
    pub hdr_type: HdrType,
    pub interlaced: bool,
    pub bit_rate: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioStream {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub channels: i64,
    pub title: Option<String>,
    pub default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleStream {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub text: bool,
}

type ProbeMediaFileFn = dyn Fn(String) -> PortFuture<Result<Probe, ProbeError>> + Send + Sync;

#[derive(Clone)]
pub struct ProbeMediaFile(Arc<ProbeMediaFileFn>);

impl ProbeMediaFile {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Probe, ProbeError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<Probe, ProbeError>> {
        (self.0)(path.to_owned())
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("ffprobe failed")]
pub struct ProbeError {
    core: ohno::OhnoCore,
}

impl ProbeError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageFile {
    pub format: ImageFormat,
    pub bytes: Bytes,
}

type ReadCachedImageFn =
    dyn Fn(CachedImage) -> PortFuture<Result<Option<ImageFile>, ImageCacheError>> + Send + Sync;

#[derive(Clone)]
pub struct ReadCachedImage(Arc<ReadCachedImageFn>);

impl ReadCachedImage {
    pub fn new(
        f: impl Fn(CachedImage) -> PortFuture<Result<Option<ImageFile>, ImageCacheError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        image: &CachedImage,
    ) -> PortFuture<Result<Option<ImageFile>, ImageCacheError>> {
        (self.0)(image.clone())
    }
}

#[derive(Clone, Debug, Default)]
pub struct EpisodeFolder {
    pub nfo: Option<FoundFile>,
    pub thumb: Option<FoundFile>,
}

type ReadEpisodeFolderFn =
    dyn Fn(String) -> PortFuture<Result<EpisodeFolder, UnreadableFile>> + Send + Sync;

#[derive(Clone)]
pub struct ReadEpisodeFolder(Arc<ReadEpisodeFolderFn>);

impl ReadEpisodeFolder {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<EpisodeFolder, UnreadableFile>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, media_file_path: &str) -> PortFuture<Result<EpisodeFolder, UnreadableFile>> {
        (self.0)(media_file_path.to_owned())
    }
}

type ReadEpisodeNfoFn =
    dyn Fn(String) -> PortFuture<Result<Vec<EpisodeNfo>, ReadNfoError>> + Send + Sync;

#[derive(Clone)]
pub struct ReadEpisodeNfo(Arc<ReadEpisodeNfoFn>);

impl ReadEpisodeNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Vec<EpisodeNfo>, ReadNfoError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<Vec<EpisodeNfo>, ReadNfoError>> {
        (self.0)(path.to_owned())
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to read the keyframes of a media file")]
pub struct ReadKeyframesError {
    core: ohno::OhnoCore,
}

impl ReadKeyframesError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type ReadKeyframesFn =
    dyn Fn(String) -> PortFuture<Result<Vec<Duration>, ReadKeyframesError>> + Send + Sync;

#[derive(Clone)]
pub struct ReadKeyframes(Arc<ReadKeyframesFn>);

impl ReadKeyframes {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Vec<Duration>, ReadKeyframesError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<Vec<Duration>, ReadKeyframesError>> {
        (self.0)(path.to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundFile {
    pub path: String,
    pub modified_at: Timestamp,
}

#[derive(Clone, Debug, Default)]
pub struct MovieFolder {
    pub nfo: Option<FoundFile>,
    pub poster: Option<FoundFile>,
    pub fanart: Option<FoundFile>,
    pub clearlogo: Option<FoundFile>,
}

type ReadMovieFolderFn =
    dyn Fn(String) -> PortFuture<Result<MovieFolder, UnreadableFile>> + Send + Sync;

#[derive(Clone)]
pub struct ReadMovieFolder(Arc<ReadMovieFolderFn>);

impl ReadMovieFolder {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<MovieFolder, UnreadableFile>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, media_file_path: &str) -> PortFuture<Result<MovieFolder, UnreadableFile>> {
        (self.0)(media_file_path.to_owned())
    }
}

type ReadMovieNfoFn = dyn Fn(String) -> PortFuture<Result<MovieNfo, ReadNfoError>> + Send + Sync;

#[derive(Clone)]
pub struct ReadMovieNfo(Arc<ReadMovieNfoFn>);

impl ReadMovieNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<MovieNfo, ReadNfoError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<MovieNfo, ReadNfoError>> {
        (self.0)(path.to_owned())
    }
}

#[derive(Debug)]
pub enum ReadNfoError {
    UnreadableFile(UnreadableFile),
    UnreadableNfo(UnreadableNfo),
}

impl From<UnreadableFile> for ReadNfoError {
    fn from(error: UnreadableFile) -> Self {
        Self::UnreadableFile(error)
    }
}

impl From<UnreadableNfo> for ReadNfoError {
    fn from(error: UnreadableNfo) -> Self {
        Self::UnreadableNfo(error)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to read an NFO file")]
pub struct UnreadableNfo {
    core: ohno::OhnoCore,
}

impl UnreadableNfo {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(ohno::Error)]
#[no_constructors]
#[display("failed to read a file")]
pub struct UnreadableFile {
    pub path: String,
    core: ohno::OhnoCore,
}

impl UnreadableFile {
    pub fn new(path: String) -> Self {
        Self {
            path,
            core: ohno::OhnoCore::default(),
        }
    }

    pub fn caused_by(path: String, source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            path,
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MovieWebhookDelivery {
    Movie { radarr_id: RadarrId },
    Ignored,
}

type ReadRadarrWebhookFn =
    dyn Fn(&[u8]) -> Result<MovieWebhookDelivery, UnreadableWebhook> + Send + Sync;

#[derive(Clone)]
pub struct ReadRadarrWebhook(Arc<ReadRadarrWebhookFn>);

impl ReadRadarrWebhook {
    pub fn new(
        f: impl Fn(&[u8]) -> Result<MovieWebhookDelivery, UnreadableWebhook> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, body: &[u8]) -> Result<MovieWebhookDelivery, UnreadableWebhook> {
        (self.0)(body)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to read a webhook delivery")]
pub struct UnreadableWebhook {
    core: ohno::OhnoCore,
}

impl UnreadableWebhook {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShowFolder {
    pub nfo: Option<FoundFile>,
    pub poster: Option<FoundFile>,
    pub fanart: Option<FoundFile>,
    pub clearlogo: Option<FoundFile>,
    pub season_posters: Vec<(i64, FoundFile)>,
}

type ReadShowFolderFn =
    dyn Fn(String, Vec<i64>) -> PortFuture<Result<ShowFolder, UnreadableFile>> + Send + Sync;

#[derive(Clone)]
pub struct ReadShowFolder(Arc<ReadShowFolderFn>);

impl ReadShowFolder {
    pub fn new(
        f: impl Fn(String, Vec<i64>) -> PortFuture<Result<ShowFolder, UnreadableFile>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        show_path: &str,
        season_numbers: Vec<i64>,
    ) -> PortFuture<Result<ShowFolder, UnreadableFile>> {
        (self.0)(show_path.to_owned(), season_numbers)
    }
}

type ReadShowNfoFn = dyn Fn(String) -> PortFuture<Result<ShowNfo, ReadNfoError>> + Send + Sync;

#[derive(Clone)]
pub struct ReadShowNfo(Arc<ReadShowNfoFn>);

impl ReadShowNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<ShowNfo, ReadNfoError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<ShowNfo, ReadNfoError>> {
        (self.0)(path.to_owned())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ShowWebhookDelivery {
    Show { sonarr_id: SonarrId },
    Ignored,
}

type ReadSonarrWebhookFn =
    dyn Fn(&[u8]) -> Result<ShowWebhookDelivery, UnreadableWebhook> + Send + Sync;

#[derive(Clone)]
pub struct ReadSonarrWebhook(Arc<ReadSonarrWebhookFn>);

impl ReadSonarrWebhook {
    pub fn new(
        f: impl Fn(&[u8]) -> Result<ShowWebhookDelivery, UnreadableWebhook> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, body: &[u8]) -> Result<ShowWebhookDelivery, UnreadableWebhook> {
        (self.0)(body)
    }
}

type ReadWebhookEventTypeFn = dyn Fn(&[u8]) -> Option<String> + Send + Sync;

#[derive(Clone)]
pub struct ReadWebhookEventType(Arc<ReadWebhookEventTypeFn>);

impl ReadWebhookEventType {
    pub fn new(f: impl Fn(&[u8]) -> Option<String> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, body: &[u8]) -> Option<String> {
        (self.0)(body)
    }
}

type ResizeImageFn =
    dyn Fn(String, i64) -> PortFuture<Result<ImageFile, ResizeImageError>> + Send + Sync;

#[derive(Clone)]
pub struct ResizeImage(Arc<ResizeImageFn>);

impl ResizeImage {
    pub fn new(
        f: impl Fn(String, i64) -> PortFuture<Result<ImageFile, ResizeImageError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str, width: i64) -> PortFuture<Result<ImageFile, ResizeImageError>> {
        (self.0)(path.to_owned(), width)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to resize an image")]
pub struct ResizeImageError {
    core: ohno::OhnoCore,
}

impl ResizeImageError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type SaveCachedImageFn =
    dyn Fn(CachedImage, ImageFile) -> PortFuture<Result<(), ImageCacheError>> + Send + Sync;

#[derive(Clone)]
pub struct SaveCachedImage(Arc<SaveCachedImageFn>);

impl SaveCachedImage {
    pub fn new(
        f: impl Fn(CachedImage, ImageFile) -> PortFuture<Result<(), ImageCacheError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        image: &CachedImage,
        file: ImageFile,
    ) -> PortFuture<Result<(), ImageCacheError>> {
        (self.0)(image.clone(), file)
    }
}

type SendEmailFn =
    dyn Fn(EmailAddress, String, String) -> PortFuture<Result<(), SendEmailError>> + Send + Sync;

#[derive(Clone)]
pub struct SendEmail(Arc<SendEmailFn>);

impl SendEmail {
    pub fn new(
        f: impl Fn(EmailAddress, String, String) -> PortFuture<Result<(), SendEmailError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        recipient: EmailAddress,
        subject: String,
        body: String,
    ) -> PortFuture<Result<(), SendEmailError>> {
        (self.0)(recipient, subject, body)
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("failed to send the email")]
pub struct SendEmailError {
    core: ohno::OhnoCore,
}

impl SendEmailError {
    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

#[derive(ohno::Error, Default)]
#[no_constructors]
#[display("the mail server refused the email")]
pub struct EmailRefused {
    core: ohno::OhnoCore,
}

impl EmailRefused {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn caused_by(source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            core: ohno::OhnoCore::from(source),
        }
    }
}

type StartHlsOutputFn = dyn Fn(HlsOutput) -> PortFuture<Result<(), HlsError>> + Send + Sync;

#[derive(Clone)]
pub struct StartHlsOutput(Arc<StartHlsOutputFn>);

impl StartHlsOutput {
    pub fn new(
        f: impl Fn(HlsOutput) -> PortFuture<Result<(), HlsError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, session: HlsOutput) -> PortFuture<Result<(), HlsError>> {
        (self.0)(session)
    }
}

type TryAcquireLockFn = dyn Fn(String) -> PortFuture<Option<LockGuard>> + Send + Sync;

#[derive(Clone)]
pub struct TryAcquireLock(Arc<TryAcquireLockFn>);

impl TryAcquireLock {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Option<LockGuard>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, key: String) -> PortFuture<Option<LockGuard>> {
        (self.0)(key)
    }
}
