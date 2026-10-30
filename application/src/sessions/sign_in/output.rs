use domain::Colour;
use domain::Symbol;
use uuid::Uuid;

pub enum Output {
    SignedIn {
        token: String,
        user_id: Uuid,
        name: String,
        colour: Colour,
        symbol: Symbol,
    },
    WrongNameOrPassword,
}
