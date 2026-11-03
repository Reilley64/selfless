use crate::browsing::Tile;
use uuid::Uuid;

#[derive(Debug)]
pub struct Output {
    pub featured: Option<FeaturedTitle>,
    pub rows: Vec<Row>,
}

#[derive(Debug)]
pub struct FeaturedTitle {
    pub tile: Tile,
    pub play_episode_id: Option<Uuid>,
}

#[derive(Debug)]
pub struct Row {
    pub kind: RowKind,
    pub tiles: Vec<Tile>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RowKind {
    ContinueWatching,
    RecentlyAdded,
    PopularNow,
    Genre(String),
}
