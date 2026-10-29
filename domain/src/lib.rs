use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Admin {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    pub password_hash: String,
}

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
