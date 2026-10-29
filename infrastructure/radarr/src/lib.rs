use application::ports::GetRadarrMovie;
use application::ports::RadarrError;
use application::ports::RadarrMovie;
use application::ports::RadarrMovieFile;
use jiff::Timestamp;
use reqwest::StatusCode;
use serde::Deserialize;
use std::path::PathBuf;
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
        let url = self
            .url
            .clone()
            .ok_or_else(|| MissingSetting::new("RADARR__URL"))?;
        let api_key = self
            .api_key
            .clone()
            .ok_or_else(|| MissingSetting::new("RADARR__API_KEY"))?;
        let client = reqwest::Client::new();
        Ok(GetRadarrMovie::new(move |radarr_id| {
            let mut url = url.clone();
            url.path_segments_mut().unwrap().pop_if_empty().extend([
                "api",
                "v3",
                "movie",
                &radarr_id.to_string(),
            ]);
            let request = client.get(url.clone()).header("X-Api-Key", &api_key);
            Box::pin(async move {
                let span = tracing::info_span!(
                    "radarr_request",
                    otel.name = "GET /api/v3/movie/{id}",
                    otel.kind = "client",
                    otel.status_code = field::Empty,
                    http.request.method = "GET",
                    http.response.status_code = field::Empty,
                    server.address = url.host_str(),
                    server.port = url.port_or_known_default(),
                    url.full = url.as_str(),
                    url.template = "/api/v3/movie/{id}",
                    error.type = field::Empty,
                );
                async {
                    let response = request.send().await.map_err(|x| failed("_OTHER", x))?;
                    let status = response.status();
                    tracing::Span::current().record("http.response.status_code", status.as_u16());
                    if status == StatusCode::NOT_FOUND {
                        return Ok(None);
                    }
                    let movie = response
                        .error_for_status()
                        .map_err(|x| failed(status.as_str(), x))?
                        .json::<Movie>()
                        .await
                        .map_err(|x| failed("_OTHER", x))?;
                    Ok(Some(RadarrMovie {
                        tmdb_id: movie.tmdb_id,
                        title: movie.title,
                        year: movie.year,
                        overview: movie.overview,
                        genres: movie.genres,
                        movie_file: movie.movie_file.map(|x| RadarrMovieFile {
                            path: x.path,
                            date_added: x.date_added,
                        }),
                    }))
                }
                .instrument(span)
                .await
            })
        }))
    }
}

fn failed(error_type: &str, error: reqwest::Error) -> RadarrError {
    let span = tracing::Span::current();
    span.record("otel.status_code", "ERROR");
    span.record("error.type", error_type);
    RadarrError::caused_by(error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Movie {
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
    path: PathBuf,
    date_added: Timestamp,
}
