use crate::Colour;
use crate::EmailAddress;
use crate::Symbol;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct User {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub name: String,
    #[unique]
    pub email: EmailAddress,
    pub password_hash: String,
    pub colour: Colour,
    pub symbol: Symbol,
    pub idempotency_key: Option<Uuid>,
}
