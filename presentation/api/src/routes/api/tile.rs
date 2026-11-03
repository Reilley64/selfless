use application::browsing::Cursor;
use application::browsing::SortKey;
use application::browsing::Tile;
use application::browsing::TileKind;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use domain::GridSort;
use serde::Deserialize;
use serde::Serialize;
use std::cmp::Reverse;
use std::str::FromStr;
use topcoat::router::error::BadRequestError;
use topcoat::router::error::bad_request;
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TilePageBody {
    tiles: Vec<TileBody>,
    next_cursor: Option<String>,
}

impl TilePageBody {
    pub(crate) fn new(tiles: Vec<Tile>, next: Option<Cursor>) -> Self {
        Self {
            tiles: tiles.into_iter().map(TileBody::from).collect(),
            next_cursor: next.map(|x| {
                let key = match x.key {
                    SortKey::Title(sort_title) => format!("t{sort_title}"),
                    SortKey::Added(Reverse(added_at)) => {
                        format!("a{}", added_at.map(|x| x.to_string()).unwrap_or_default())
                    }
                    SortKey::Released(Reverse(released)) => {
                        format!("r{}", released.map(|x| x.to_string()).unwrap_or_default())
                    }
                };
                URL_SAFE_NO_PAD.encode(format!("{}{key}", x.id))
            }),
        }
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SortParam {
    #[default]
    Title,
    Added,
    Released,
}

impl From<SortParam> for GridSort {
    fn from(sort: SortParam) -> Self {
        match sort {
            SortParam::Title => GridSort::Title,
            SortParam::Added => GridSort::Added,
            SortParam::Released => GridSort::Released,
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct CursorParam(Cursor);

impl CursorParam {
    pub(crate) fn for_sort(&self, sort: SortParam) -> Result<Cursor, BadRequestError> {
        match (&self.0.key, sort) {
            (SortKey::Title(_), SortParam::Title)
            | (SortKey::Added(_), SortParam::Added)
            | (SortKey::Released(_), SortParam::Released) => Ok(self.0.clone()),
            _ => Err(bad_request("the cursor belongs to another sort")),
        }
    }
}

#[ohno::error]
#[display("the cursor is not one the server issued")]
pub(crate) struct InvalidCursor;

impl TryFrom<String> for CursorParam {
    type Error = InvalidCursor;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let decoded = URL_SAFE_NO_PAD
            .decode(text)
            .ok()
            .and_then(|x| String::from_utf8(x).ok())
            .ok_or_else(InvalidCursor::new)?;
        let id_length = Uuid::nil().to_string().len();
        let (id, key) = decoded
            .split_at_checked(id_length)
            .ok_or_else(InvalidCursor::new)?;
        let key = match key.split_at_checked(1) {
            Some(("t", sort_title)) => SortKey::Title(sort_title.to_owned()),
            Some(("a", added_at)) => SortKey::Added(Reverse(parse_optional(added_at)?)),
            Some(("r", released)) => SortKey::Released(Reverse(parse_optional(released)?)),
            _ => return Err(InvalidCursor::new()),
        };
        Ok(Self(Cursor {
            key,
            id: id.parse().map_err(|_| InvalidCursor::new())?,
        }))
    }
}

fn parse_optional<T: FromStr>(text: &str) -> Result<Option<T>, InvalidCursor> {
    if text.is_empty() {
        return Ok(None);
    }
    text.parse().map(Some).map_err(|_| InvalidCursor::new())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TileBody {
    id: Uuid,
    kind: Kind,
    title: String,
    year: Option<i64>,
    certification: Option<String>,
    runtime_minutes: Option<i64>,
    season_count: Option<i64>,
    genres: Vec<String>,
    plot: Option<String>,
    images: TileImagesBody,
    progress: Option<f64>,
    watched: bool,
    episode: (),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Movie,
    Show,
}

#[derive(Serialize)]
struct TileImagesBody {
    fanart: (),
    logo: (),
    thumb: (),
}

impl From<Tile> for TileBody {
    fn from(tile: Tile) -> Self {
        let (kind, runtime_minutes, season_count) = match tile.kind {
            TileKind::Movie { runtime_minutes } => (Kind::Movie, runtime_minutes, None),
            TileKind::Show { season_count } => (Kind::Show, None, Some(season_count)),
        };
        Self {
            id: tile.id,
            kind,
            title: tile.title,
            year: tile.year,
            certification: tile.certification,
            runtime_minutes,
            season_count,
            genres: tile.genres,
            plot: tile.plot,
            images: TileImagesBody {
                fanart: (),
                logo: (),
                thumb: (),
            },
            progress: None,
            watched: false,
            episode: (),
        }
    }
}
