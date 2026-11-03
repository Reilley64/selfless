use crate::browsing::MovieWatch;
use crate::browsing::TileImages;
use domain::Movie;
use domain::Show;
use jiff::Timestamp;
use jiff::civil::Date;
use std::cmp::Reverse;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Tile {
    pub id: Uuid,
    pub title: String,
    pub year: Option<i64>,
    pub certification: Option<String>,
    pub genres: Vec<String>,
    pub plot: Option<String>,
    pub kind: TileKind,
    pub images: TileImages,
    pub progress: Option<f64>,
    pub watched: bool,
}

impl Tile {
    pub(crate) fn movie(movie: Movie, images: TileImages, watch: MovieWatch) -> Self {
        Self {
            id: movie.id,
            title: movie.title,
            year: Some(movie.year),
            certification: movie.certification,
            genres: movie.genres,
            plot: movie.overview,
            kind: TileKind::Movie {
                runtime_minutes: movie.runtime_minutes,
            },
            images,
            progress: watch.progress,
            watched: watch.state.watched,
        }
    }

    pub(crate) fn show(show: Show, season_count: i64, images: TileImages) -> Self {
        Self {
            id: show.id,
            title: show.title,
            year: show.year,
            certification: show.certification,
            genres: show.genres,
            plot: show.overview,
            kind: TileKind::Show { season_count },
            images,
            progress: None,
            watched: false,
        }
    }
}

#[derive(Clone, Debug)]
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
