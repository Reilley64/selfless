use crate::browsing::TileImages;
use jiff::Timestamp;
use jiff::civil::Date;
use std::cmp::Reverse;
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
    pub key: SortKey,
    pub id: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortKey {
    Title(String),
    Added(Reverse<Option<Timestamp>>),
    Released(Reverse<Option<Date>>),
}
