use crate::browsing::Tile;
use crate::browsing::TileImages;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Movie(Box<Movie>),
    NotFound,
}

#[derive(Debug)]
pub struct Movie {
    pub id: Uuid,
    pub title: String,
    pub year: i64,
    pub certification: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub plot: Option<String>,
    pub studio: Option<String>,
    pub original_title: Option<String>,
    pub directors: Vec<String>,
    pub cast: Vec<CastMember>,
    pub images: TileImages,
    pub more_like_this: Vec<Tile>,
}

#[derive(Debug, PartialEq)]
pub struct CastMember {
    pub name: String,
    pub role: Option<String>,
}
