use domain::ClientKind;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub sessions: Vec<Session>,
}

#[derive(Debug)]
pub struct Session {
    pub id: Uuid,
    pub client_kind: ClientKind,
    pub device_name: String,
    pub signed_in_at: Timestamp,
    pub last_used_at: Timestamp,
}
