use crate::Source;
use jiff::SignedDuration;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[table = "inboxes"]
#[index(delivered_at, updated_at)]
pub struct Inbox {
    #[key]
    #[auto]
    pub id: Uuid,
    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
    pub delivered_at: Option<Timestamp>,
    pub source: Source,
    pub body: Vec<u8>,
    pub received_at: Timestamp,
    pub traceparent: Option<String>,
}

impl Inbox {
    const KEPT_FOR: SignedDuration = SignedDuration::from_hours(30 * 24);

    pub fn expired_filter(now: Timestamp) -> toasty::stmt::Expr<bool> {
        Self::fields()
            .delivered_at()
            .is_some()
            .and(Self::fields().received_at().le(now - Self::KEPT_FOR))
    }
}
