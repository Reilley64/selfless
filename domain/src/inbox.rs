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

    fn received_cutoff(now: Timestamp) -> Timestamp {
        now - Self::KEPT_FOR
    }

    pub fn expired_filter(now: Timestamp) -> toasty::stmt::Expr<bool> {
        Self::fields()
            .delivered_at()
            .is_some()
            .and(Self::fields().received_at().le(Self::received_cutoff(now)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    #[test]
    fn a_delivered_delivery_is_kept_for_thirty_days_after_it_was_received() {
        assert_eq!(
            Inbox::received_cutoff(at("2026-06-01T12:00:00Z")),
            at("2026-05-02T12:00:00Z")
        );
    }
}
