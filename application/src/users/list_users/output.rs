use domain::Colour;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub users: Vec<User>,
}

#[derive(Debug)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub colour: Colour,
    pub created_at: Timestamp,
    pub signed_in_devices: usize,
    pub last_used_at: Option<Timestamp>,
}
