use domain::ArtworkKind;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub seasons: Vec<Season>,
}

#[derive(Debug)]
pub struct Season {
    pub id: Uuid,
    pub number: i64,
    pub artwork: Vec<Artwork>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Artwork {
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}
