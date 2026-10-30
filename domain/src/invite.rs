use crate::EmailAddress;
use jiff::SignedDuration;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Invite {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub email: EmailAddress,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn invite() -> Invite {
        Invite {
            id: Uuid::nil(),
            email: EmailAddress::new("noor@household.example".to_owned())
                .expect("the address is valid"),
            token_hash: Vec::new(),
            admin_id: Uuid::nil(),
            sent_at: at("2026-05-25T12:00:00Z"),
            expires_at: at("2026-06-01T12:00:00Z"),
        }
    }

    #[test]
    fn an_invite_has_expired_at_its_expiry_time() {
        assert!(invite().expired(at("2026-06-01T12:00:00Z")));
    }

    #[test]
    fn an_invite_has_not_expired_before_its_expiry_time() {
        assert!(!invite().expired(at("2026-06-01T11:59:59Z")));
    }
}
