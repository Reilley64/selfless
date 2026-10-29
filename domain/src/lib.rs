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
