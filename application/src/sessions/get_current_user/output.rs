use domain::Colour;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    SignedIn {
        id: Uuid,
        name: String,
        colour: Colour,
    },
    NotSignedIn,
}
