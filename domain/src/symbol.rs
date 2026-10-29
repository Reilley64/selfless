#[derive(
    Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, serde::Serialize, serde::Deserialize,
)]
#[column(rename_all = "camelCase")]
#[serde(rename_all = "camelCase")]
pub enum Symbol {
    Ring,
    Triangle,
    Star,
    Waves,
}

impl Symbol {
    pub const ALL: [Self; 4] = [Self::Ring, Self::Triangle, Self::Star, Self::Waves];
}
