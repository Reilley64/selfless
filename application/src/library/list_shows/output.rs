use domain::ArtworkKind;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub shows: Vec<Show>,
}

#[derive(Debug)]
pub struct Show {
    pub id: Uuid,
    pub title: String,
    pub sort_title: Option<String>,
    pub year: Option<i64>,
    pub genres: Vec<String>,
    pub overview: Option<String>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub rating: Option<f64>,
    pub actors: Vec<Actor>,
    pub artwork: Vec<Artwork>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Actor {
    pub name: String,
    pub role: Option<String>,
    pub order: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Artwork {
    pub kind: ArtworkKind,
    pub path: String,
    pub modified_at: Timestamp,
}
