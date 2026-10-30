use domain::Colour;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub id: Uuid,
    pub name: String,
    pub colour: Colour,
}
