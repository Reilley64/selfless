use bytes::Bytes;
use jiff::Timestamp;
use jiff::civil::Date;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

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

pub struct SonarrShow {
    pub tvdb_id: i64,
    pub title: String,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
    pub episodes: Vec<SonarrEpisode>,
}

pub struct SonarrEpisode {
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
