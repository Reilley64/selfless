use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub(crate) struct UserBody {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) colour: Colour,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl From<domain::Colour> for Colour {
    fn from(colour: domain::Colour) -> Self {
        match colour {
            domain::Colour::Amber => Self::Amber,
            domain::Colour::Blue => Self::Blue,
            domain::Colour::Pink => Self::Pink,
            domain::Colour::Emerald => Self::Emerald,
        }
    }
}
