mod subtitle_files;

pub use subtitle_files::list_subtitle_files;

use application::ports::EpisodeFolder;
use application::ports::EpisodeNfo;
use application::ports::FindMediaFile;
use application::ports::FoundFile;
use application::ports::FoundMediaFile;
use application::ports::MovieFolder;
use application::ports::MovieNfo;
use application::ports::NfoActor;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadNfoError;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::ShowFolder;
use application::ports::ShowNfo;
use application::ports::UnreadableFile;
use application::ports::UnreadableNfo;
use jiff::Timestamp;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::io::ErrorKind;
use std::path::Path;

pub fn find_media_file() -> FindMediaFile {
    FindMediaFile::new(|path| {
        Box::pin(async move {
            let Some((size, modified_at)) = stat(Path::new(&path)).await? else {
                return Ok(None);
            };
            Ok(Some(FoundMediaFile {
                size: i64::try_from(size).map_err(|x| UnreadableFile::caused_by(path, x))?,
                modified_at,
            }))
        })
    })
}

pub fn read_movie_folder() -> ReadMovieFolder {
    ReadMovieFolder::new(|media_file_path| {
        Box::pin(async move {
            let media_file = Path::new(&media_file_path);
            let folder = media_file.parent().unwrap_or(Path::new(""));
            Ok(MovieFolder {
                nfo: found(&media_file.with_extension("nfo")).await?,
                poster: found(&folder.join("poster.jpg")).await?,
                fanart: found(&folder.join("fanart.jpg")).await?,
                clearlogo: found(&folder.join("clearlogo.png")).await?,
            })
        })
    })
}

async fn found(path: &Path) -> Result<Option<FoundFile>, UnreadableFile> {
    Ok(stat(path).await?.map(|(_, modified_at)| FoundFile {
        path: path.to_string_lossy().into_owned(),
        modified_at,
    }))
}

async fn stat(path: &Path) -> Result<Option<(u64, Timestamp)>, UnreadableFile> {
    let unreadable = |error: Box<dyn std::error::Error + Send + Sync>| {
        UnreadableFile::caused_by(path.to_string_lossy().into_owned(), error)
    };
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unreadable(error.into())),
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    let modified_at = metadata
        .modified()
        .map_err(|x| unreadable(x.into()))
        .and_then(|x| Timestamp::try_from(x).map_err(|x| unreadable(x.into())))?;
    let modified_at = Timestamp::from_microsecond(modified_at.as_microsecond())
        .map_err(|x| unreadable(x.into()))?;
    Ok(Some((metadata.len(), modified_at)))
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
                if let Some(file) = found(&folder.join(name)).await? {
                    season_posters.push((number, file));
                }
            }
            Ok(ShowFolder {
                nfo: found(&folder.join("tvshow.nfo")).await?,
                poster: found(&folder.join("poster.jpg")).await?,
                fanart: found(&folder.join("fanart.jpg")).await?,
                clearlogo: found(&folder.join("clearlogo.png")).await?,
                season_posters,
            })
        })
    })
}

pub fn read_episode_folder() -> ReadEpisodeFolder {
    ReadEpisodeFolder::new(|media_file_path| {
        Box::pin(async move {
            let media_file = Path::new(&media_file_path);
            let stem = media_file.file_stem().unwrap_or_default().to_string_lossy();
            Ok(EpisodeFolder {
                nfo: found(&media_file.with_extension("nfo")).await?,
                thumb: found(&media_file.with_file_name(format!("{stem}-thumb.jpg"))).await?,
            })
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

async fn read_nfo<T: DeserializeOwned>(path: &str) -> Result<T, ReadNfoError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|x| UnreadableFile::caused_by(path.to_owned(), x))?;
    let text = String::from_utf8(bytes).map_err(UnreadableNfo::caused_by)?;
    Ok(quick_xml::de::from_str(&clean_up(&text)).map_err(UnreadableNfo::caused_by)?)
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

#[cfg(test)]
mod tests {
    use super::find_media_file;
    use super::read_episode_folder;
    use super::read_episode_nfo;
    use super::read_movie_folder;
    use super::read_movie_nfo;
    use super::read_show_folder;
    use super::read_show_nfo;
    use application::ports::EpisodeNfo;
    use application::ports::FoundFile;
    use application::ports::FoundMediaFile;
    use application::ports::NfoActor;
    use application::ports::ReadNfoError;
    use jiff::Timestamp;
    use jiff::civil::date;
    use std::time::SystemTime;
    use tempfile::TempDir;

    const MODIFIED_AT: &str = "2026-10-29T10:00:00Z";

    fn fixture(path: &str) -> String {
        format!("{}/../../test/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))
    }

    fn actors(actors: &[NfoActor]) -> Vec<(&str, Option<&str>, Option<i64>)> {
        actors
            .iter()
            .map(|x| (x.name.as_str(), x.role.as_deref(), x.order))
            .collect()
    }

    type Episode<'a> = (Option<i64>, Option<&'a str>, Option<i64>, Option<i64>);

    fn episodes(episodes: &[EpisodeNfo]) -> Vec<Episode<'_>> {
        episodes
            .iter()
            .map(|x| {
                (
                    x.sonarr_id,
                    x.title.as_deref(),
                    x.season_number,
                    x.episode_number,
                )
            })
            .collect()
    }

    struct Folder(TempDir);

    impl Folder {
        fn new() -> Self {
            Self(TempDir::new().unwrap())
        }

        fn path(&self, relative_path: &str) -> String {
            self.0
                .path()
                .join(relative_path)
                .to_str()
                .unwrap()
                .to_owned()
        }

        fn write(&self, relative_path: &str, contents: &str) -> String {
            let path = self.0.path().join(relative_path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, contents).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(SystemTime::from(modified_at()))
                .unwrap();
            self.path(relative_path)
        }

        fn found(&self, relative_path: &str) -> Option<FoundFile> {
            Some(FoundFile {
                path: self.path(relative_path),
                modified_at: modified_at(),
            })
        }
    }

    fn modified_at() -> Timestamp {
        MODIFIED_AT.parse().unwrap()
    }

    #[tokio::test]
    async fn read_movie_nfo_maps_every_field_the_themoviedb_rating_and_the_actors_in_order() {
        let nfo = read_movie_nfo()
            .call(&fixture("radarr/nfo/movie.nfo"))
            .await
            .unwrap();

        assert_eq!(nfo.title.as_deref(), Some("Arrival"));
        assert_eq!(nfo.sort_title.as_deref(), Some("arrival"));
        assert_eq!(nfo.original_title.as_deref(), Some("Arrival"));
        assert_eq!(nfo.year, Some(2016));
        assert_eq!(
            nfo.plot.as_deref(),
            Some(
                "When twelve mysterious spacecraft appear around the world, linguistics professor \
                 Louise Banks is tasked with interpreting the language of the apparent alien visitors."
            )
        );
        assert_eq!(nfo.runtime_minutes, Some(116));
        assert_eq!(nfo.certification.as_deref(), Some("PG-13"));
        assert_eq!(nfo.premiered, Some(date(2016, 11, 10)));
        assert_eq!(nfo.studio.as_deref(), Some("FilmNation Entertainment"));
        assert_eq!(nfo.genres, ["Drama", "Science Fiction", "Mystery"]);
        assert_eq!(nfo.rating, Some(7.6));
        assert_eq!(nfo.directors, ["Denis Villeneuve"]);
        assert_eq!(
            actors(&nfo.actors),
            [
                ("Amy Adams", Some("Louise Banks"), Some(0)),
                ("Jeremy Renner", Some("Ian Donnelly"), Some(1)),
                ("Forest Whitaker", Some("Colonel Weber"), Some(2)),
            ]
        );
    }

    #[tokio::test]
    async fn a_movie_nfo_with_trailing_url_lines_is_read() {
        let nfo = read_movie_nfo()
            .call(&fixture("radarr/nfo/movie-with-urls.nfo"))
            .await
            .unwrap();

        assert_eq!(nfo.title.as_deref(), Some("Arrival"));
        assert_eq!(nfo.actors.len(), 3);
    }

    #[tokio::test]
    async fn a_movie_nfo_that_cannot_be_parsed_is_an_unreadable_nfo() {
        let nfo = read_movie_nfo()
            .call(&fixture("radarr/nfo/movie-unreadable.nfo"))
            .await;

        assert!(matches!(nfo, Err(ReadNfoError::UnreadableNfo(_))));
    }

    #[tokio::test]
    async fn a_movie_nfo_that_cannot_be_read_is_an_unreadable_file() {
        let folder = Folder::new();
        let not_a_folder = folder.write("Arrival (2016)", "");
        let path = format!("{not_a_folder}/Arrival (2016) Bluray-1080p.nfo");

        let nfo = read_movie_nfo().call(&path).await;

        assert!(matches!(nfo, Err(ReadNfoError::UnreadableFile(error)) if error.path == path));
    }

    #[tokio::test]
    async fn read_show_nfo_maps_every_field_and_the_actors_in_order() {
        let nfo = read_show_nfo()
            .call(&fixture("sonarr/nfo/tvshow.nfo"))
            .await
            .unwrap();

        assert_eq!(nfo.title.as_deref(), Some("Andor"));
        assert_eq!(nfo.sort_title.as_deref(), Some("andor"));
        assert!(
            nfo.plot
                .as_deref()
                .unwrap()
                .ends_with("destined to turn him into a rebel hero.")
        );
        assert_eq!(nfo.certification.as_deref(), Some("TV-14"));
        assert_eq!(nfo.premiered, Some(date(2022, 9, 21)));
        assert_eq!(nfo.end_date, Some(date(2025, 5, 13)));
        assert_eq!(nfo.status.as_deref(), Some("Ended"));
        assert_eq!(nfo.network.as_deref(), Some("Disney+"));
        assert_eq!(
            nfo.genres,
            ["Action", "Adventure", "Drama", "Science Fiction"]
        );
        assert_eq!(nfo.rating, Some(8.4));
        assert_eq!(
            actors(&nfo.actors),
            [
                ("Diego Luna", Some("Cassian Andor"), Some(0)),
                ("Stellan Skarsgård", Some("Luthen Rael"), Some(1)),
                ("Genevieve O'Reilly", Some("Mon Mothma"), Some(2)),
            ]
        );
    }

    #[tokio::test]
    async fn read_episode_nfo_maps_the_sonarr_id_and_every_field() {
        let nfos = read_episode_nfo()
            .call(&fixture("sonarr/nfo/episode.nfo"))
            .await
            .unwrap();

        let [nfo] = nfos.as_slice() else {
            panic!("one episode, not {nfos:?}");
        };
        assert_eq!(nfo.sonarr_id, Some(1101));
        assert_eq!(nfo.title.as_deref(), Some("Kassa"));
        assert_eq!(nfo.season_number, Some(1));
        assert_eq!(nfo.episode_number, Some(1));
        assert_eq!(nfo.air_date, Some(date(2022, 9, 21)));
        assert_eq!(
            nfo.plot.as_deref(),
            Some(
                "Cassian Andor looks for his missing sister on the industrial planet of Morlana One."
            )
        );
        assert_eq!(nfo.rating, Some(7.5));
    }

    #[tokio::test]
    async fn an_episode_nfo_with_two_episodes_gives_both() {
        let nfos = read_episode_nfo()
            .call(&fixture("sonarr/nfo/two-episodes.nfo"))
            .await
            .unwrap();

        assert_eq!(
            episodes(&nfos),
            [
                (Some(1012), Some("Half Loop"), Some(1), Some(2)),
                (Some(1013), Some("In Perpetuity"), Some(1), Some(3)),
            ]
        );
        assert_eq!(
            nfos.iter()
                .map(|x| (x.air_date, x.rating))
                .collect::<Vec<_>>(),
            [
                (Some(date(2022, 2, 18)), Some(8.1)),
                (Some(date(2022, 2, 25)), Some(8.3)),
            ]
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_keeps_its_own_numbers_and_may_have_no_sonarr_id() {
        let renumbered = read_episode_nfo()
            .call(&fixture("sonarr/nfo/renumbered-episode.nfo"))
            .await
            .unwrap();
        let special = read_episode_nfo()
            .call(&fixture("sonarr/nfo/special-without-sonarr-id.nfo"))
            .await
            .unwrap();

        assert_eq!(
            episodes(&renumbered),
            [(Some(1021), Some("Hello, Ms. Cobel"), Some(3), Some(9))]
        );
        assert_eq!(
            episodes(&special),
            [(None, Some("Inside the Episode"), Some(0), Some(1))]
        );
    }

    #[tokio::test]
    async fn an_episode_nfo_that_cannot_be_parsed_is_an_unreadable_nfo() {
        let nfos = read_episode_nfo()
            .call(&fixture("sonarr/nfo/unreadable.nfo"))
            .await;

        assert!(matches!(nfos, Err(ReadNfoError::UnreadableNfo(_))));
    }

    #[tokio::test]
    async fn read_movie_folder_finds_the_nfo_beside_the_file_and_the_artwork_with_their_times() {
        let folder = Folder::new();
        let media_file = folder.write("Arrival (2016)/Arrival (2016) Bluray-1080p.mkv", "");
        for name in [
            "Arrival (2016) Bluray-1080p.nfo",
            "poster.jpg",
            "fanart.jpg",
            "clearlogo.png",
            "banner.jpg",
            "Arrival (2016) Bluray-1080p-poster.jpg",
        ] {
            folder.write(&format!("Arrival (2016)/{name}"), "");
        }

        let movie_folder = read_movie_folder().call(&media_file).await.unwrap();

        assert_eq!(
            movie_folder.nfo,
            folder.found("Arrival (2016)/Arrival (2016) Bluray-1080p.nfo")
        );
        assert_eq!(
            movie_folder.poster,
            folder.found("Arrival (2016)/poster.jpg")
        );
        assert_eq!(
            movie_folder.fanart,
            folder.found("Arrival (2016)/fanart.jpg")
        );
        assert_eq!(
            movie_folder.clearlogo,
            folder.found("Arrival (2016)/clearlogo.png")
        );
    }

    #[tokio::test]
    async fn read_show_folder_finds_the_nfo_the_artwork_and_the_posters_of_the_seasons_asked_for() {
        let folder = Folder::new();
        for name in [
            "tvshow.nfo",
            "poster.jpg",
            "fanart.jpg",
            "clearlogo.png",
            "season01-poster.jpg",
            "season-specials-poster.jpg",
            "season03-poster.jpg",
        ] {
            folder.write(&format!("Severance/{name}"), "");
        }

        let show_folder = read_show_folder()
            .call(&folder.path("Severance"), vec![0, 1, 2])
            .await
            .unwrap();

        assert_eq!(show_folder.nfo, folder.found("Severance/tvshow.nfo"));
        assert_eq!(show_folder.poster, folder.found("Severance/poster.jpg"));
        assert_eq!(show_folder.fanart, folder.found("Severance/fanart.jpg"));
        assert_eq!(
            show_folder.clearlogo,
            folder.found("Severance/clearlogo.png")
        );
        assert_eq!(
            show_folder.season_posters,
            [
                (
                    0,
                    folder
                        .found("Severance/season-specials-poster.jpg")
                        .unwrap()
                ),
                (1, folder.found("Severance/season01-poster.jpg").unwrap()),
            ]
        );
    }

    #[tokio::test]
    async fn read_episode_folder_finds_the_nfo_and_the_thumb_beside_the_file() {
        let folder = Folder::new();
        let media_file = folder.write("Andor/Season 01/Andor - S01E01 - Kassa.mkv", "");
        folder.write("Andor/Season 01/Andor - S01E01 - Kassa.nfo", "");
        folder.write("Andor/Season 01/Andor - S01E01 - Kassa-thumb.jpg", "");

        let episode_folder = read_episode_folder().call(&media_file).await.unwrap();

        assert_eq!(
            episode_folder.nfo,
            folder.found("Andor/Season 01/Andor - S01E01 - Kassa.nfo")
        );
        assert_eq!(
            episode_folder.thumb,
            folder.found("Andor/Season 01/Andor - S01E01 - Kassa-thumb.jpg")
        );
    }

    #[tokio::test]
    async fn find_media_file_gives_the_size_and_modification_time() {
        let folder = Folder::new();
        let media_file = folder.write("Arrival (2016)/Arrival (2016) Bluray-1080p.mkv", "video");

        assert_eq!(
            find_media_file().call(&media_file).await.unwrap(),
            Some(FoundMediaFile {
                size: 5,
                modified_at: modified_at(),
            })
        );
    }

    #[tokio::test]
    async fn find_media_file_gives_none_for_a_missing_file() {
        let folder = Folder::new();

        assert_eq!(
            find_media_file()
                .call(&folder.path("Arrival (2016)/Arrival (2016) Bluray-1080p.mkv"))
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn find_media_file_gives_a_read_error_for_a_path_it_cannot_read() {
        let folder = Folder::new();
        let not_a_folder = folder.write("Arrival (2016)", "");
        let media_file = format!("{not_a_folder}/Arrival (2016) Bluray-1080p.mkv");

        let error = find_media_file().call(&media_file).await.unwrap_err();

        assert_eq!(error.path, media_file);
    }

    #[tokio::test]
    async fn read_movie_folder_gives_a_read_error_for_a_file_it_cannot_read() {
        let folder = Folder::new();
        let not_a_folder = folder.write("Arrival (2016)", "");

        let error = read_movie_folder()
            .call(&format!("{not_a_folder}/Arrival (2016) Bluray-1080p.mkv"))
            .await
            .unwrap_err();

        assert_eq!(
            error.path,
            format!("{not_a_folder}/Arrival (2016) Bluray-1080p.nfo")
        );
    }
}
