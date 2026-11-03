use crate::OutboxData;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[table = "outboxes"]
#[index(delivered_at, updated_at)]
pub struct Outbox {
    #[key]
    #[auto]
    pub id: Uuid,
    #[auto]
    pub created_at: Timestamp,
    #[auto]
    pub updated_at: Timestamp,
    pub delivered_at: Option<Timestamp>,
    pub data: OutboxData,
    pub traceparent: Option<String>,
}
