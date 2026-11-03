use domain::ArtworkKind;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub episodes: Vec<Episode>,
}

#[derive(Debug)]
pub struct Episode {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub air_date: Option<Date>,
    pub overview: Option<String>,
    pub added_at: Option<Timestamp>,
    pub media_file_id: Option<Uuid>,
    pub rating: Option<f64>,
    pub artwork: Vec<Artwork>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Artwork {
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}
