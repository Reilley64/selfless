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
