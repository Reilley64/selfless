use application::ports::CheckRadarrKodiMetadata;
use application::ports::GetRadarrMovie;
use application::ports::ListRadarrMovies;
use application::ports::RadarrError;
use application::ports::RadarrMovie;
use application::ports::RadarrMovieFile;
use jiff::Timestamp;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tracing::Instrument;
use tracing::field;
use url::Url;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct RadarrSettings {
    pub url: Option<Url>,
    pub api_key: Option<String>,
    pub webhook_password: Option<String>,
}

#[ohno::error]
#[display("missing setting: {name}")]
pub struct MissingSetting {
    name: &'static str,
}

impl RadarrSettings {
    pub fn webhook_password(&self) -> Result<String, MissingSetting> {
        self.webhook_password
            .clone()
            .ok_or_else(|| MissingSetting::new("RADARR__WEBHOOK_PASSWORD"))
    }

    pub fn get_movie(&self) -> Result<GetRadarrMovie, MissingSetting> {
        let api = self.api()?;
        Ok(GetRadarrMovie::new(move |radarr_id| {
            let api = api.clone();
            Box::pin(async move {
                match api
                    .get::<Movie>("/api/v3/movie/{id}", &["movie", &radarr_id.to_string()])
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
                    .get::<Vec<Movie>>("/api/v3/movie", &["movie"])
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
            Box::pin(async move {
                Ok(api
                    .get::<Vec<MetadataConsumer>>("/api/v3/metadata", &["metadata"])
                    .await
                    .map_err(RadarrError::caused_by)?
                    .iter()
                    .any(|x| x.implementation == "XbmcMetadata" && x.enable))
            })
        }))
    }

    fn api(&self) -> Result<Api, MissingSetting> {
        Ok(Api {
            client: reqwest::Client::new(),
            url: self
                .url
                .clone()
                .ok_or_else(|| MissingSetting::new("RADARR__URL"))?,
            api_key: self
                .api_key
                .clone()
                .ok_or_else(|| MissingSetting::new("RADARR__API_KEY"))?,
        })
    }
}

#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    url: Url,
    api_key: String,
}

impl Api {
    async fn get<T: DeserializeOwned>(
        &self,
        template: &'static str,
        path: &[&str],
    ) -> Result<T, reqwest::Error> {
        let mut url = self.url.clone();
        url.path_segments_mut()
            .expect("RADARR__URL is an http or https URL")
            .pop_if_empty()
            .extend(["api", "v3"])
            .extend(path);
        let span = tracing::info_span!(
            "radarr_request",
            otel.name = format!("GET {template}"),
            otel.kind = "client",
            otel.status_code = field::Empty,
            http.request.method = "GET",
            url.full = url.as_str(),
            url.template = template,
            server.address = url.host_str(),
            server.port = url.port_or_known_default(),
            http.response.status_code = field::Empty,
            error.type = field::Empty,
        );
        async {
            let response = self
                .client
                .get(url.clone())
                .header("X-Api-Key", &self.api_key)
                .send()
                .await?;
            span.record("http.response.status_code", response.status().as_u16());
            response.error_for_status()?.json::<T>().await
        }
        .instrument(span.clone())
        .await
        .inspect_err(|error| {
            span.record("otel.status_code", "ERROR");
            span.record(
                "error.type",
                error
                    .status()
                    .map_or_else(|| "_OTHER".to_owned(), |x| x.as_u16().to_string()),
            );
        })
    }
}

#[derive(Deserialize)]
struct MetadataConsumer {
    implementation: String,
    enable: bool,
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
