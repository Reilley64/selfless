use crate::EmailAddress;
use crate::InviteEmailStatus;
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
    pub token_hash: Option<Vec<u8>>,
    pub admin_id: Uuid,
    pub email_status: InviteEmailStatus,
    pub expires_at: Option<Timestamp>,
}

impl Invite {
    pub const VALID_DAYS: i64 = 7;
    pub const VALID_FOR: SignedDuration = SignedDuration::from_hours(Self::VALID_DAYS * 24);

    pub fn expires_at(sent_at: Timestamp) -> Timestamp {
        sent_at + Self::VALID_FOR
    }

    pub fn expired(&self, now: Timestamp) -> bool {
        self.expires_at.is_none_or(|x| now >= x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn invite(expires_at: Option<Timestamp>) -> Invite {
        Invite {
            id: Uuid::nil(),
            email: EmailAddress::new("noor@household.example".to_owned())
                .expect("the address is valid"),
            token_hash: Some(Vec::new()),
            admin_id: Uuid::nil(),
            email_status: InviteEmailStatus::Sent {
                sent_at: at("2026-05-25T12:00:00Z"),
            },
            expires_at,
        }
    }

    #[test]
    fn an_invite_has_expired_at_its_expiry_time() {
        assert!(invite(Some(at("2026-06-01T12:00:00Z"))).expired(at("2026-06-01T12:00:00Z")));
    }

    #[test]
    fn an_invite_has_not_expired_before_its_expiry_time() {
        assert!(!invite(Some(at("2026-06-01T12:00:00Z"))).expired(at("2026-06-01T11:59:59Z")));
    }

    #[test]
    fn an_invite_without_an_expiry_has_expired() {
        assert!(invite(None).expired(at("2026-05-25T12:00:00Z")));
    }

    #[test]
    fn an_invite_expires_7_days_after_its_email_is_sent() {
        assert_eq!(
            Invite::expires_at(at("2026-05-25T12:00:00Z")),
            at("2026-06-01T12:00:00Z")
        );
    }
}
