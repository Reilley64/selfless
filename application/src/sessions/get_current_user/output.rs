use domain::ClientKind;
use domain::Colour;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    SignedIn {
        id: Uuid,
        session_id: Uuid,
        client_kind: ClientKind,
        name: String,
        colour: Colour,
    },
    NotSignedIn,
}
