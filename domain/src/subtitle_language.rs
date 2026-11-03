#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleLanguage {
    pub language: String,
    pub hearing_impaired: bool,
    pub forced: bool,
}
