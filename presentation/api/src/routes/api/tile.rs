use crate::routes::api::image::ImageBody;
use application::browsing::Cursor;
use application::browsing::Tile;
use application::browsing::TileKind;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde::Serialize;
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
            next_cursor: next.map(|x| URL_SAFE_NO_PAD.encode(format!("{}{}", x.id, x.sort_title))),
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct CursorParam(pub(crate) Cursor);

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
        let (id, sort_title) = decoded
            .split_at_checked(id_length)
            .ok_or_else(InvalidCursor::new)?;
        Ok(Self(Cursor {
            sort_title: sort_title.to_owned(),
            id: id.parse().map_err(|_| InvalidCursor::new())?,
        }))
    }
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
    fanart: Option<ImageBody>,
    logo: Option<ImageBody>,
    thumb: Option<ImageBody>,
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
                fanart: tile.images.fanart.map(ImageBody::from),
                logo: tile.images.logo.map(ImageBody::from),
                thumb: tile.images.thumb.map(ImageBody::from),
            },
            progress: None,
            watched: false,
            episode: (),
        }
    }
}
