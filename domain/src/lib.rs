use jiff::Timestamp;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum Source {
    Radarr,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Radarr => "radarr",
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
}

#[derive(Debug, toasty::Model)]
pub struct SourceState {
    #[key]
    pub source: Source,
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
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
    pub visible: bool,
}
