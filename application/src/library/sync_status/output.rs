use domain::ReconcileResult;
use domain::SyncProblemKind;
use jiff::Timestamp;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub radarr: SourceStatus,
    pub sonarr: SourceStatus,
    pub problems: Vec<Problem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: SyncProblemKind,
    pub item: ProblemItem,
    pub path: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProblemItem {
    Movie {
        title: String,
        id: Uuid,
    },
    Show {
        title: String,
        id: Uuid,
    },
    Episode {
        show: String,
        season: i64,
        number: i64,
        title: String,
        id: Uuid,
    },
}

#[derive(Debug)]
pub struct SourceStatus {
    pub last_api_call: Option<ApiCall>,
    pub kodi_metadata: Option<bool>,
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
    pub reconcile_progress: Option<ReconcileProgress>,
    pub last_reconcile: Option<LastReconcile>,
    pub held_removal: Option<HeldRemoval>,
    pub in_library: InLibraryCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiCall {
    Succeeded,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconcileProgress {
    pub imported: usize,
    pub total: Option<usize>,
}

#[derive(Debug)]
pub struct LastReconcile {
    pub started_at: Timestamp,
    pub duration: Duration,
    pub result: ReconcileResult,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HeldRemoval {
    pub count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InLibraryCounts {
    Movies(u64),
    Shows { shows: u64, episodes: u64 },
}
