use crate::Admin;
use jiff::SignedDuration;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct AdminSession {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub admin_id: Uuid,
    #[belongs_to]
    pub admin: Deferred<Admin>,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl AdminSession {
    const IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(12);
    const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);

    pub fn idle_cutoff(now: Timestamp) -> Timestamp {
        now - Self::IDLE_TIMEOUT
    }

    pub fn expired(&self, now: Timestamp) -> bool {
        self.last_used_at <= Self::idle_cutoff(now)
    }

    pub fn last_used_is_stale(&self, now: Timestamp) -> bool {
        now.duration_since(self.last_used_at) >= Self::LAST_USED_PRECISION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-06-01T12:00:00Z";

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn admin_session(last_used_at: &str) -> AdminSession {
        AdminSession {
            id: Uuid::nil(),
            admin_id: Uuid::nil(),
            admin: Deferred::default(),
            token_hash: Vec::new(),
            signed_in_at: at("2026-06-01T00:00:00Z"),
            last_used_at: at(last_used_at),
        }
    }

    #[test]
    fn an_admin_session_idle_for_twelve_hours_has_expired() {
        assert!(admin_session("2026-06-01T00:00:00Z").expired(at(NOW)));
    }

    #[test]
    fn an_admin_session_idle_for_less_than_twelve_hours_has_not_expired() {
        assert!(!admin_session("2026-06-01T00:00:01Z").expired(at(NOW)));
    }

    #[test]
    fn the_idle_cutoff_is_twelve_hours_before_now() {
        assert_eq!(
            AdminSession::idle_cutoff(at(NOW)),
            at("2026-06-01T00:00:00Z")
        );
    }

    #[test]
    fn last_used_an_hour_ago_is_stale() {
        assert!(admin_session("2026-06-01T11:00:00Z").last_used_is_stale(at(NOW)));
    }

    #[test]
    fn last_used_less_than_an_hour_ago_is_not_stale() {
        assert!(!admin_session("2026-06-01T11:00:01Z").last_used_is_stale(at(NOW)));
    }
}
