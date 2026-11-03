use crate::browsing::Tile;
use crate::browsing::TileImages;
use domain::PlayVerb;
use uuid::Uuid;

#[derive(Debug)]
pub enum Output {
    Show(Box<Show>),
    NotFound,
}

#[derive(Debug)]
pub struct Show {
    pub id: Uuid,
    pub title: String,
    pub year: Option<i64>,
    pub certification: Option<String>,
    pub status: Option<String>,
    pub network: Option<String>,
    pub season_count: i64,
    pub genres: Vec<String>,
    pub rating: Option<f64>,
    pub plot: Option<String>,
    pub cast: Vec<CastMember>,
    pub images: TileImages,
    pub seasons: Vec<Season>,
    pub play: Play,
    pub more_like_this: Vec<Tile>,
}

#[derive(Debug, PartialEq)]
pub struct CastMember {
    pub name: String,
    pub role: Option<String>,
}

#[derive(Debug)]
pub struct Season {
    pub id: Uuid,
    pub number: i64,
    pub episode_count: i64,
}

#[derive(Debug)]
pub struct Play {
    pub episode_id: Uuid,
    pub episode_number: i64,
    pub episode_title: String,
    pub media_file_id: Option<Uuid>,
    pub season_id: Uuid,
    pub season_number: i64,
    pub verb: PlayVerb,
}
