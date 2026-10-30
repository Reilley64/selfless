use domain::Colour;
use domain::Symbol;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    SignedIn {
        id: Uuid,
        name: String,
        colour: Colour,
        symbol: Symbol,
    },
    NotSignedIn,
}
