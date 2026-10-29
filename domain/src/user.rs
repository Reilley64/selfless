use crate::Colour;
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
    pub email: String,
    pub password_hash: String,
    pub colour: Colour,
    pub symbol: Symbol,
}
