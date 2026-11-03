use domain::Colour;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    SignedIn {
        id: Uuid,
        session_id: Uuid,
        name: String,
        colour: Colour,
    },
    NotSignedIn,
}
