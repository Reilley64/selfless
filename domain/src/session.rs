use crate::ClientKind;
use crate::User;
use jiff::SignedDuration;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Session {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub user_id: Uuid,
    #[belongs_to]
    pub user: Deferred<User>,
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
