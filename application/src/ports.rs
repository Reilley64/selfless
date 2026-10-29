use bytes::Bytes;
use jiff::Timestamp;
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

type SendEmailFn =
    dyn Fn(String, String, String) -> PortFuture<Result<(), SendEmailError>> + Send + Sync;

#[derive(Clone)]
pub struct SendEmail(Arc<SendEmailFn>);

impl SendEmail {
    pub fn new(
        f: impl Fn(String, String, String) -> PortFuture<Result<(), SendEmailError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self(Arc::new(f))
    }

    pub fn call(
        &self,
        recipient: String,
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
