use crate::browsing::TileImages;
use uuid::Uuid;

#[derive(Debug)]
pub struct Tile {
    pub id: Uuid,
    pub title: String,
    pub year: Option<i64>,
    pub certification: Option<String>,
    pub genres: Vec<String>,
    pub plot: Option<String>,
    pub kind: TileKind,
    pub images: TileImages,
}

#[derive(Debug)]
pub enum TileKind {
    Movie { runtime_minutes: Option<i64> },
    Show { season_count: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub sort_title: String,
    pub id: Uuid,
}
