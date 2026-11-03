use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub invites: Vec<Invite>,
}

#[derive(Debug)]
pub struct Invite {
    pub id: Uuid,
    pub email: String,
    pub email_status: EmailStatus,
    pub expires_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailStatus {
    Queued { queued_at: Timestamp },
    Sent { sent_at: Timestamp },
    Refused { refused_at: Timestamp },
}
