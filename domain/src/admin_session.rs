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
    pub const IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(12);
    pub const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);
}
