use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub admins: Vec<Admin>,
}

#[derive(Debug)]
pub struct Admin {
    pub id: Uuid,
    pub name: String,
    pub created_at: Timestamp,
    pub last_signed_in_at: Option<Timestamp>,
}
