use domain::Colour;
use domain::Symbol;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub id: Uuid,
    pub name: String,
    pub colour: Colour,
    pub symbol: Symbol,
}
