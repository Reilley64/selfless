use crate::ClientKind;
use crate::User;
use jiff::SignedDuration;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[unique(user_id, client_id)]
pub struct Session {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub user_id: Uuid,
    #[belongs_to]
    pub user: Deferred<User>,
    pub client_id: Uuid,
    pub client_kind: ClientKind,
    pub device_name: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}

impl Session {
    const WEB_IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(30 * 24);
    const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);

    pub fn web_idle_cutoff(now: Timestamp) -> Timestamp {
        now - Self::WEB_IDLE_TIMEOUT
    }

    pub fn expired(&self, now: Timestamp) -> bool {
        self.client_kind == ClientKind::Web && self.last_used_at <= Self::web_idle_cutoff(now)
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

    fn session(client_kind: ClientKind, last_used_at: &str) -> Session {
        Session {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            user: Deferred::default(),
            client_id: Uuid::nil(),
            client_kind,
            device_name: "Living room".to_owned(),
            token_hash: Vec::new(),
            signed_in_at: at("2026-01-01T00:00:00Z"),
            last_used_at: at(last_used_at),
        }
    }

    #[test]
    fn a_web_session_idle_for_thirty_days_has_expired() {
        assert!(session(ClientKind::Web, "2026-05-02T12:00:00Z").expired(at(NOW)));
    }

    #[test]
    fn a_web_session_idle_for_less_than_thirty_days_has_not_expired() {
        assert!(!session(ClientKind::Web, "2026-05-02T12:00:01Z").expired(at(NOW)));
    }

    #[test]
    fn a_tv_session_never_expires() {
        for client_kind in [ClientKind::AppleTv, ClientKind::GoogleTv] {
            assert!(!session(client_kind, "2025-06-01T12:00:00Z").expired(at(NOW)));
        }
    }

    #[test]
    fn the_web_idle_cutoff_is_thirty_days_before_now() {
        assert_eq!(
            Session::web_idle_cutoff(at(NOW)),
            at("2026-05-02T12:00:00Z")
        );
    }

    #[test]
    fn last_used_an_hour_ago_is_stale() {
        assert!(session(ClientKind::Web, "2026-06-01T11:00:00Z").last_used_is_stale(at(NOW)));
    }

    #[test]
    fn last_used_less_than_an_hour_ago_is_not_stale() {
        assert!(!session(ClientKind::Web, "2026-06-01T11:00:01Z").last_used_is_stale(at(NOW)));
    }
}
