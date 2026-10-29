mod subtitle_files;

use application::ports::EpisodeFolder;
use application::ports::EpisodeNfo;
use application::ports::FoundFile;
use application::ports::MovieFolder;
use application::ports::MovieNfo;
use application::ports::NfoActor;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::ShowFolder;
use application::ports::ShowNfo;
use application::ports::UnreadableNfo;
use jiff::Timestamp;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::Path;

pub use subtitle_files::list_subtitle_files;

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

pub fn read_show_folder() -> ReadShowFolder {
    ReadShowFolder::new(|show_path, season_numbers| {
        Box::pin(async move {
            let folder = Path::new(&show_path);
            let mut season_posters = Vec::new();
            for number in season_numbers {
                let name = match number {
                    0 => "season-specials-poster.jpg".to_owned(),
                    number => format!("season{number:02}-poster.jpg"),
                };
                if let Some(file) = found(&folder.join(name)).await {
                    season_posters.push((number, file));
                }
            }
            ShowFolder {
                nfo: found(&folder.join("tvshow.nfo")).await,
                poster: found(&folder.join("poster.jpg")).await,
                fanart: found(&folder.join("fanart.jpg")).await,
                clearlogo: found(&folder.join("clearlogo.png")).await,
                season_posters,
            }
        })
    })
}

pub fn read_episode_folder() -> ReadEpisodeFolder {
    ReadEpisodeFolder::new(|media_file_path| {
        Box::pin(async move {
            let media_file = Path::new(&media_file_path);
            let stem = media_file.file_stem().unwrap_or_default().to_string_lossy();
            EpisodeFolder {
                nfo: found(&media_file.with_extension("nfo")).await,
                thumb: found(&media_file.with_file_name(format!("{stem}-thumb.jpg"))).await,
            }
        })
    })
}

pub fn read_movie_nfo() -> ReadMovieNfo {
    ReadMovieNfo::new(|path| {
        Box::pin(async move {
            let document: MovieDocument = read_nfo(&path).await?;
            Ok(document.movie.into())
        })
    })
}

pub fn read_show_nfo() -> ReadShowNfo {
    ReadShowNfo::new(|path| {
        Box::pin(async move {
            let document: ShowDocument = read_nfo(&path).await?;
            Ok(document.tvshow.into())
        })
    })
}

pub fn read_episode_nfo() -> ReadEpisodeNfo {
    ReadEpisodeNfo::new(|path| {
        Box::pin(async move {
            let document: EpisodeDocument = read_nfo(&path).await?;
            Ok(document
                .episodedetails
                .into_iter()
                .map(EpisodeNfo::from)
                .collect())
        })
    })
}

async fn read_nfo<T: DeserializeOwned>(path: &str) -> Result<T, UnreadableNfo> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(UnreadableNfo::caused_by)?;
    let text = String::from_utf8(bytes).map_err(UnreadableNfo::caused_by)?;
    quick_xml::de::from_str(&clean_up(&text)).map_err(UnreadableNfo::caused_by)
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

#[derive(Deserialize)]
struct ShowDocument {
    tvshow: NfoShow,
}

#[derive(Deserialize)]
struct NfoShow {
    title: Option<String>,
    sorttitle: Option<String>,
    plot: Option<String>,
    mpaa: Option<String>,
    premiered: Option<String>,
    enddate: Option<String>,
    status: Option<String>,
    #[serde(default)]
    studio: Vec<String>,
    #[serde(default)]
    genre: Vec<String>,
    rating: Option<String>,
    #[serde(default)]
    actor: Vec<Actor>,
}

#[derive(Deserialize)]
struct EpisodeDocument {
    #[serde(default)]
    episodedetails: Vec<NfoEpisode>,
}

#[derive(Deserialize)]
struct NfoEpisode {
    title: Option<String>,
    season: Option<String>,
    episode: Option<String>,
    aired: Option<String>,
    plot: Option<String>,
    rating: Option<String>,
    #[serde(default)]
    uniqueid: Vec<UniqueId>,
}

#[derive(Deserialize)]
struct UniqueId {
    #[serde(rename = "@type")]
    kind: Option<String>,
    #[serde(rename = "$text")]
    value: Option<String>,
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
            actors: actors(movie.actor),
        }
    }
}

impl From<NfoShow> for ShowNfo {
    fn from(show: NfoShow) -> Self {
        Self {
            title: text(show.title),
            sort_title: text(show.sorttitle),
            plot: text(show.plot),
            certification: text(show.mpaa),
            premiered: parsed(show.premiered),
            end_date: parsed(show.enddate),
            status: text(show.status),
            network: texts(show.studio).into_iter().next(),
            genres: texts(show.genre),
            rating: parsed(show.rating),
            actors: actors(show.actor),
        }
    }
}

impl From<NfoEpisode> for EpisodeNfo {
    fn from(episode: NfoEpisode) -> Self {
        Self {
            sonarr_id: episode
                .uniqueid
                .into_iter()
                .find(|x| x.kind.as_deref() == Some("sonarr"))
                .and_then(|x| parsed(x.value)),
            title: text(episode.title),
            season_number: parsed(episode.season),
            episode_number: parsed(episode.episode),
            air_date: parsed(episode.aired),
            plot: text(episode.plot),
            rating: parsed(episode.rating),
        }
    }
}

fn actors(actors: Vec<Actor>) -> Vec<NfoActor> {
    actors
        .into_iter()
        .filter_map(|x| {
            Some(NfoActor {
                name: text(x.name)?,
                role: text(x.role),
                order: parsed(x.order),
            })
        })
        .collect()
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
