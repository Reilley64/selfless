use application::ports::FoundFile;
use application::ports::MovieFolder;
use application::ports::MovieNfo;
use application::ports::NfoActor;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::UnreadableNfo;
use jiff::Timestamp;
use serde::Deserialize;
use std::path::Path;

pub fn read_movie_folder() -> ReadMovieFolder {
    ReadMovieFolder::new(|media_file_path| {
        Box::pin(async move {
            let media_file = Path::new(&media_file_path);
            let folder = media_file.parent().unwrap_or(Path::new(""));
            MovieFolder {
                nfo: found(&media_file.with_extension("nfo")).await,
                poster: found(&folder.join("poster.jpg")).await,
                fanart: found(&folder.join("fanart.jpg")).await,
                clearlogo: found(&folder.join("clearlogo.png")).await,
            }
        })
    })
}

async fn found(path: &Path) -> Option<FoundFile> {
    let metadata = tokio::fs::metadata(path)
        .await
        .ok()
        .filter(|x| x.is_file())?;
    let modified_at = Timestamp::try_from(metadata.modified().ok()?).ok()?;
    Some(FoundFile {
        path: path.to_str()?.to_owned(),
        modified_at: Timestamp::from_microsecond(modified_at.as_microsecond()).ok()?,
    })
}

pub fn read_movie_nfo() -> ReadMovieNfo {
    ReadMovieNfo::new(|path| {
        Box::pin(async move {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(UnreadableNfo::caused_by)?;
            let text = String::from_utf8(bytes).map_err(UnreadableNfo::caused_by)?;
            let document: MovieDocument =
                quick_xml::de::from_str(&clean_up(&text)).map_err(UnreadableNfo::caused_by)?;
            Ok(document.movie.into())
        })
    })
}

fn clean_up(text: &str) -> String {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let text = match text.strip_prefix("<?xml") {
        Some(declared) => declared.split_once("?>").map_or(declared, |(_, rest)| rest),
        None => text,
    };
    let text = text.rfind('>').map_or(text, |end| &text[..=end]);
    format!("<nfo>{text}</nfo>")
}

#[derive(Deserialize)]
struct MovieDocument {
    movie: NfoMovie,
}

#[derive(Deserialize)]
struct NfoMovie {
    title: Option<String>,
    sorttitle: Option<String>,
    originaltitle: Option<String>,
    year: Option<String>,
    plot: Option<String>,
    runtime: Option<String>,
    mpaa: Option<String>,
    premiered: Option<String>,
    #[serde(default)]
    studio: Vec<String>,
    #[serde(default)]
    genre: Vec<String>,
    ratings: Option<Ratings>,
    #[serde(default)]
    director: Vec<String>,
    #[serde(default)]
    actor: Vec<Actor>,
}

#[derive(Deserialize)]
struct Ratings {
    #[serde(default)]
    rating: Vec<Rating>,
}

#[derive(Deserialize)]
struct Rating {
    #[serde(rename = "@name")]
    name: Option<String>,
    value: Option<String>,
}

#[derive(Deserialize)]
struct Actor {
    name: Option<String>,
    role: Option<String>,
    order: Option<String>,
}

impl From<NfoMovie> for MovieNfo {
    fn from(movie: NfoMovie) -> Self {
        Self {
            title: text(movie.title),
            sort_title: text(movie.sorttitle),
            original_title: text(movie.originaltitle),
            year: parsed(movie.year),
            plot: text(movie.plot),
            runtime_minutes: parsed(movie.runtime),
            certification: text(movie.mpaa),
            premiered: parsed(movie.premiered),
            studio: texts(movie.studio).into_iter().next(),
            genres: texts(movie.genre),
            rating: movie
                .ratings
                .into_iter()
                .flat_map(|x| x.rating)
                .find(|x| x.name.as_deref() == Some("themoviedb"))
                .and_then(|x| parsed(x.value)),
            directors: texts(movie.director),
            actors: movie
                .actor
                .into_iter()
                .filter_map(|x| {
                    Some(NfoActor {
                        name: text(x.name)?,
                        role: text(x.role),
                        order: parsed(x.order),
                    })
                })
                .collect(),
        }
    }
}

fn text(value: Option<String>) -> Option<String> {
    value.map(|x| x.trim().to_owned()).filter(|x| !x.is_empty())
}

fn texts(values: Vec<String>) -> Vec<String> {
    values.into_iter().filter_map(|x| text(Some(x))).collect()
}

fn parsed<T: std::str::FromStr>(value: Option<String>) -> Option<T> {
    text(value)?.parse().ok()
}
