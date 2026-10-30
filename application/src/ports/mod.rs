use bytes::Bytes;
use jiff::Timestamp;
use std::error::Error;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

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
