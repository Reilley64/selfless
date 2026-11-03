use domain::ArtworkKind;
use jiff::Timestamp;
use jiff::civil::Date;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub movies: Vec<Movie>,
}

#[derive(Debug)]
pub struct Movie {
    pub id: Uuid,
    pub title: String,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub year: i64,
    pub overview: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub certification: Option<String>,
    pub premiered: Option<Date>,
    pub studio: Option<String>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub directors: Vec<String>,
    pub actors: Vec<Actor>,
    pub artwork: Vec<Artwork>,
    pub added_at: Option<Timestamp>,
    pub media_file_id: Option<Uuid>,
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
