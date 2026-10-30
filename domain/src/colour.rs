#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl Colour {
    pub const ALL: [Self; 4] = [Self::Amber, Self::Blue, Self::Pink, Self::Emerald];
}
