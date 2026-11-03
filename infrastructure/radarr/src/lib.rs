mod webhook;

pub use webhook::read_webhook;

use application::ports::CheckRadarrKodiMetadata;
use application::ports::GetRadarrMovie;
use application::ports::ListRadarrMovies;
use application::ports::RadarrError;
use application::ports::RadarrMovie;
use application::ports::RadarrMovieFile;
use jiff::Timestamp;
use reqwest::StatusCode;
use serde::Deserialize;
use servarr::Api;
use servarr::MissingSetting;
use servarr::setting;
use url::Url;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct RadarrSettings {
    pub url: Option<Url>,
    pub api_key: Option<String>,
    pub webhook_password: Option<String>,
}

impl RadarrSettings {
    pub fn webhook_password(&self) -> Result<String, MissingSetting> {
        setting(&self.webhook_password, "RADARR__WEBHOOK_PASSWORD")
    }

    pub fn get_movie(&self) -> Result<GetRadarrMovie, MissingSetting> {
        let api = self.api()?;
        Ok(GetRadarrMovie::new(move |radarr_id| {
            let api = api.clone();
            Box::pin(async move {
                match api
                    .get::<Movie>(
                        "/api/v3/movie/{id}",
                        &["movie", &radarr_id.to_string()],
                        None,
                    )
                    .await
                {
                    Err(error) if error.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
                    movie => Ok(Some(movie.map_err(RadarrError::caused_by)?.into())),
                }
            })
        }))
    }

    pub fn list_movies(&self) -> Result<ListRadarrMovies, MissingSetting> {
        let api = self.api()?;
        Ok(ListRadarrMovies::new(move || {
            let api = api.clone();
            Box::pin(async move {
                Ok(api
                    .get::<Vec<Movie>>("/api/v3/movie", &["movie"], None)
                    .await
                    .map_err(RadarrError::caused_by)?
                    .into_iter()
                    .map(RadarrMovie::from)
                    .collect())
            })
        }))
    }

    pub fn check_kodi_metadata(&self) -> Result<CheckRadarrKodiMetadata, MissingSetting> {
        let api = self.api()?;
        Ok(CheckRadarrKodiMetadata::new(move || {
            let api = api.clone();
            Box::pin(async move { api.kodi_metadata().await.map_err(RadarrError::caused_by) })
        }))
    }

    fn api(&self) -> Result<Api, MissingSetting> {
        Ok(Api::new(
            setting(&self.url, "RADARR__URL")?,
            setting(&self.api_key, "RADARR__API_KEY")?,
        ))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Movie {
    id: i64,
    tmdb_id: i64,
    title: String,
    year: i64,
    overview: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    movie_file: Option<MovieFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovieFile {
    path: String,
    date_added: Timestamp,
}

impl From<Movie> for RadarrMovie {
    fn from(movie: Movie) -> Self {
        RadarrMovie {
            radarr_id: movie.id,
            tmdb_id: movie.tmdb_id,
            title: movie.title,
            year: movie.year,
            overview: movie.overview,
            genres: movie.genres,
            movie_file: movie.movie_file.map(|x| RadarrMovieFile {
                path: x.path,
                date_added: x.date_added,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RadarrSettings;
    use jiff::Timestamp;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    const API_KEY: &str = "radarr-api-key";

    async fn radarr(url_path: &str, response: ResponseTemplate) -> (MockServer, RadarrSettings) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(url_path))
            .respond_with(response)
            .mount(&server)
            .await;
        let settings = RadarrSettings {
            url: Some(server.uri().parse().unwrap()),
            api_key: Some(API_KEY.to_owned()),
            webhook_password: None,
        };
        (server, settings)
    }

    fn json(status: u16, body: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_raw(body.to_owned(), "application/json")
    }

    #[tokio::test]
    async fn get_movie_maps_radarrs_movie_and_its_file() {
        let (_server, settings) = radarr(
            "/api/v3/movie/42",
            json(
                200,
                include_str!("../../../test/fixtures/radarr/api/movie.json"),
            ),
        )
        .await;

        let movie = settings
            .get_movie()
            .unwrap()
            .call(42)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(movie.radarr_id, 42);
        assert_eq!(movie.tmdb_id, 329865);
        assert_eq!(movie.title, "Arrival");
        assert_eq!(movie.year, 2016);
        assert_eq!(movie.genres, ["Drama", "Science Fiction", "Mystery"]);
        assert_eq!(
            movie.overview.as_deref(),
            Some(
                "Taking place after alien crafts land around the world, an expert linguist is \
                 recruited by the military to determine whether they come in peace or are a threat."
            )
        );
        let movie_file = movie.movie_file.unwrap();
        assert_eq!(
            movie_file.path,
            "/mnt/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv"
        );
        assert_eq!(
            movie_file.date_added,
            "2026-10-29T10:00:00Z".parse::<Timestamp>().unwrap()
        );
    }

    #[tokio::test]
    async fn a_movie_without_a_file_has_no_movie_file() {
        let (_server, settings) = radarr(
            "/api/v3/movie/42",
            json(
                200,
                include_str!("../../../test/fixtures/radarr/api/movie-without-file.json"),
            ),
        )
        .await;

        let movie = settings
            .get_movie()
            .unwrap()
            .call(42)
            .await
            .unwrap()
            .unwrap();

        assert!(movie.movie_file.is_none());
    }

    #[tokio::test]
    async fn a_movie_radarr_does_not_have_is_none() {
        let (_server, settings) = radarr(
            "/api/v3/movie/42",
            json(
                404,
                include_str!("../../../test/fixtures/radarr/api/movie-not-found.json"),
            ),
        )
        .await;

        let movie = settings.get_movie().unwrap().call(42).await.unwrap();

        assert!(movie.is_none());
    }

    #[tokio::test]
    async fn a_server_error_from_radarr_is_an_error() {
        let (_server, settings) = radarr("/api/v3/movie/42", ResponseTemplate::new(500)).await;

        let movie = settings.get_movie().unwrap().call(42).await;

        assert!(movie.is_err());
    }

    #[tokio::test]
    async fn requests_send_the_api_key() {
        let (server, settings) = radarr(
            "/api/v3/movie/42",
            json(
                200,
                include_str!("../../../test/fixtures/radarr/api/movie.json"),
            ),
        )
        .await;

        settings.get_movie().unwrap().call(42).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].headers["X-Api-Key"], API_KEY);
    }

    #[tokio::test]
    async fn list_movies_maps_every_listed_movie() {
        let (_server, settings) = radarr(
            "/api/v3/movie",
            json(
                200,
                include_str!("../../../test/fixtures/radarr/api/movies.json"),
            ),
        )
        .await;

        let movies = settings.list_movies().unwrap().call().await.unwrap();

        assert_eq!(
            movies
                .iter()
                .map(|x| (x.radarr_id, x.tmdb_id, x.title.as_str(), x.year))
                .collect::<Vec<_>>(),
            [
                (42, 329865, "Arrival", 2016),
                (43, 438631, "Dune", 2021),
                (44, 335984, "Blade Runner 2049", 2017),
                (45, 273481, "Sicario", 2015),
            ]
        );
        assert_eq!(
            movies[1].movie_file.as_ref().unwrap().path,
            "/mnt/media/Movies/Dune (2021)/Dune (2021) Bluray-2160p.mkv"
        );
    }
}
