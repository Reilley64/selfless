#[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed)]
#[column(rename_all = "camelCase")]
pub enum Colour {
    Amber,
    Blue,
    Pink,
    Emerald,
}

impl Colour {
    pub const ALL: [Self; 4] = [Self::Amber, Self::Blue, Self::Pink, Self::Emerald];
}
