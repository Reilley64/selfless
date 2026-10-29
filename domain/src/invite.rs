use jiff::SignedDuration;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Invite {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: String,
    #[unique]
    pub token_hash: Vec<u8>,
    pub admin_id: Uuid,
    pub sent_at: Timestamp,
    pub expires_at: Timestamp,
}

impl Invite {
    pub const VALID_FOR: SignedDuration = SignedDuration::from_hours(7 * 24);

    pub fn expired(&self, now: Timestamp) -> bool {
        now >= self.expires_at
    }
}
