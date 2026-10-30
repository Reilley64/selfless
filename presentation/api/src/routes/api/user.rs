use serde::Serialize;
use uuid::Uuid;

#[derive(Serialize)]
pub(crate) struct UserBody {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) colour: Colour,
    pub(crate) symbol: Symbol,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Symbol {
    Ring,
    Triangle,
    Star,
    Waves,
}

impl From<domain::Symbol> for Symbol {
    fn from(symbol: domain::Symbol) -> Self {
        match symbol {
            domain::Symbol::Ring => Self::Ring,
            domain::Symbol::Triangle => Self::Triangle,
            domain::Symbol::Star => Self::Star,
            domain::Symbol::Waves => Self::Waves,
        }
    }
}
