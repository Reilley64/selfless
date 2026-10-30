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
    pub sent_at: Timestamp,
    pub expires_at: Timestamp,
}
