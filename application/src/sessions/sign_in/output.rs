use domain::Colour;
use uuid::Uuid;

pub enum Output {
    SignedIn {
        token: String,
        user_id: Uuid,
        name: String,
        colour: Colour,
    },
    WrongNameOrPassword,
}
