use crate::ReconcileResult;
use jiff::Timestamp;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
pub struct LastReconcile {
    started_at: Timestamp,
    duration_ms: i64,
    result: ReconcileResult,
}

impl LastReconcile {
    pub fn new(started_at: Timestamp, duration: Duration, result: ReconcileResult) -> Self {
        Self {
            started_at,
            duration_ms: i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
            result,
        }
    }

    pub fn started_at(&self) -> Timestamp {
        self.started_at
    }

    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.duration_ms.unsigned_abs())
    }

    pub fn result(&self) -> ReconcileResult {
        self.result
    }
}
