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
