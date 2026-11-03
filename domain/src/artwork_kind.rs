#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
pub enum ArtworkKind {
    Poster,
    Fanart,
    Clearlogo,
    Thumb,
}
