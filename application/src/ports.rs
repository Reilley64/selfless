use bytes::Bytes;
use jiff::Timestamp;
use jiff::civil::Date;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub type PortFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TelemetrySignal {
    Traces,
    Metrics,
    Logs,
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

#[derive(Clone, Debug)]
pub struct RadarrMovie {
    pub radarr_id: i64,
    pub tmdb_id: i64,
    pub title: String,
    pub year: i64,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    pub movie_file: Option<RadarrMovieFile>,
}

#[derive(Clone, Debug)]
pub struct RadarrMovieFile {
    pub path: String,
    pub date_added: Timestamp,
}

type GetRadarrMovieFn =
    dyn Fn(i64) -> PortFuture<Result<Option<RadarrMovie>, RadarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetRadarrMovie(Arc<GetRadarrMovieFn>);

impl GetRadarrMovie {
    pub fn new(
        f: impl Fn(i64) -> PortFuture<Result<Option<RadarrMovie>, RadarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, radarr_id: i64) -> PortFuture<Result<Option<RadarrMovie>, RadarrError>> {
        (self.0)(radarr_id)
    }
}

type ListRadarrMoviesFn =
    dyn Fn() -> PortFuture<Result<Vec<RadarrMovie>, RadarrError>> + Send + Sync;

#[derive(Clone)]
pub struct ListRadarrMovies(Arc<ListRadarrMoviesFn>);

impl ListRadarrMovies {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<Vec<RadarrMovie>, RadarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<Vec<RadarrMovie>, RadarrError>> {
        (self.0)()
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

pub struct SonarrSeries {
    pub sonarr_id: i64,
    pub tvdb_id: i64,
    pub path: String,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
}

pub struct SonarrShow {
    pub series: SonarrSeries,
    pub episodes: Vec<SonarrEpisode>,
}

pub struct SonarrEpisode {
    pub sonarr_id: i64,
    pub tvdb_id: i64,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub episode_file: Option<SonarrEpisodeFile>,
}

pub struct SonarrEpisodeFile {
    pub path: String,
    pub date_added: Timestamp,
}

type GetSonarrShowFn =
    dyn Fn(i64) -> PortFuture<Result<Option<SonarrShow>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetSonarrShow(Arc<GetSonarrShowFn>);

impl GetSonarrShow {
    pub fn new(
        f: impl Fn(i64) -> PortFuture<Result<Option<SonarrShow>, SonarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, sonarr_id: i64) -> PortFuture<Result<Option<SonarrShow>, SonarrError>> {
        (self.0)(sonarr_id)
    }
}

type ListSonarrSeriesFn =
    dyn Fn() -> PortFuture<Result<Vec<SonarrSeries>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct ListSonarrSeries(Arc<ListSonarrSeriesFn>);

impl ListSonarrSeries {
    pub fn new(
        f: impl Fn() -> PortFuture<Result<Vec<SonarrSeries>, SonarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self) -> PortFuture<Result<Vec<SonarrSeries>, SonarrError>> {
        (self.0)()
    }
}

type GetSonarrEpisodesFn =
    dyn Fn(i64) -> PortFuture<Result<Vec<SonarrEpisode>, SonarrError>> + Send + Sync;

#[derive(Clone)]
pub struct GetSonarrEpisodes(Arc<GetSonarrEpisodesFn>);

impl GetSonarrEpisodes {
    pub fn new(
        f: impl Fn(i64) -> PortFuture<Result<Vec<SonarrEpisode>, SonarrError>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, sonarr_id: i64) -> PortFuture<Result<Vec<SonarrEpisode>, SonarrError>> {
        (self.0)(sonarr_id)
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

type ReadMovieFolderFn = dyn Fn(String) -> PortFuture<MovieFolder> + Send + Sync;

#[derive(Clone)]
pub struct ReadMovieFolder(Arc<ReadMovieFolderFn>);

impl ReadMovieFolder {
    pub fn new(f: impl Fn(String) -> PortFuture<MovieFolder> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, media_file_path: &str) -> PortFuture<MovieFolder> {
        (self.0)(media_file_path.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct MovieNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub plot: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
    pub actors: Vec<NfoActor>,
}

#[derive(Clone, Debug)]
pub struct NfoActor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}

type ReadMovieNfoFn = dyn Fn(String) -> PortFuture<Result<MovieNfo, UnreadableNfo>> + Send + Sync;

#[derive(Clone)]
pub struct ReadMovieNfo(Arc<ReadMovieNfoFn>);

impl ReadMovieNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<MovieNfo, UnreadableNfo>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<MovieNfo, UnreadableNfo>> {
        (self.0)(path.to_owned())
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

#[derive(Clone, Debug, Default)]
pub struct ShowFolder {
    pub nfo: Option<FoundFile>,
    pub poster: Option<FoundFile>,
    pub fanart: Option<FoundFile>,
    pub clearlogo: Option<FoundFile>,
    pub season_posters: Vec<(i64, FoundFile)>,
}

type ReadShowFolderFn = dyn Fn(String, Vec<i64>) -> PortFuture<ShowFolder> + Send + Sync;

#[derive(Clone)]
pub struct ReadShowFolder(Arc<ReadShowFolderFn>);

impl ReadShowFolder {
    pub fn new(
        f: impl Fn(String, Vec<i64>) -> PortFuture<ShowFolder> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, show_path: &str, season_numbers: Vec<i64>) -> PortFuture<ShowFolder> {
        (self.0)(show_path.to_owned(), season_numbers)
    }
}

#[derive(Clone, Debug)]
pub struct ShowNfo {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub plot: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub actors: Vec<NfoActor>,
}

type ReadShowNfoFn = dyn Fn(String) -> PortFuture<Result<ShowNfo, UnreadableNfo>> + Send + Sync;

#[derive(Clone)]
pub struct ReadShowNfo(Arc<ReadShowNfoFn>);

impl ReadShowNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<ShowNfo, UnreadableNfo>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<ShowNfo, UnreadableNfo>> {
        (self.0)(path.to_owned())
    }
}

#[derive(Clone, Debug, Default)]
pub struct EpisodeFolder {
    pub nfo: Option<FoundFile>,
    pub thumb: Option<FoundFile>,
}

type ReadEpisodeFolderFn = dyn Fn(String) -> PortFuture<EpisodeFolder> + Send + Sync;

#[derive(Clone)]
pub struct ReadEpisodeFolder(Arc<ReadEpisodeFolderFn>);

impl ReadEpisodeFolder {
    pub fn new(f: impl Fn(String) -> PortFuture<EpisodeFolder> + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, media_file_path: &str) -> PortFuture<EpisodeFolder> {
        (self.0)(media_file_path.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct EpisodeNfo {
    pub sonarr_id: Option<i64>,
    pub title: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub air_date: Option<Date>,
    pub plot: Option<String>,
    pub rating: Option<f64>,
}

type ReadEpisodeNfoFn =
    dyn Fn(String) -> PortFuture<Result<Vec<EpisodeNfo>, UnreadableNfo>> + Send + Sync;

#[derive(Clone)]
pub struct ReadEpisodeNfo(Arc<ReadEpisodeNfoFn>);

impl ReadEpisodeNfo {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Vec<EpisodeNfo>, UnreadableNfo>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(&self, path: &str) -> PortFuture<Result<Vec<EpisodeNfo>, UnreadableNfo>> {
        (self.0)(path.to_owned())
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdrType {
    Sdr,
    Hdr10,
    Hlg,
    DolbyVision,
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

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubtitleFile {
    pub path: String,
    pub language: Option<String>,
    pub hearing_impaired: bool,
    pub forced: bool,
}

type ListSubtitleFilesFn =
    dyn Fn(String) -> PortFuture<Result<Vec<SubtitleFile>, ListSubtitleFilesError>> + Send + Sync;

#[derive(Clone)]
pub struct ListSubtitleFiles(Arc<ListSubtitleFilesFn>);

impl ListSubtitleFiles {
    pub fn new(
        f: impl Fn(String) -> PortFuture<Result<Vec<SubtitleFile>, ListSubtitleFilesError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        media_file_path: &str,
    ) -> PortFuture<Result<Vec<SubtitleFile>, ListSubtitleFilesError>> {
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
