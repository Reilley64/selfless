use crate::LastReconcile;
use crate::Source;
use jiff::Timestamp;

#[derive(Debug, toasty::Model)]
pub struct SourceState {
    #[key]
    pub source: Source,
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
    pub last_reconcile: Option<LastReconcile>,
}
