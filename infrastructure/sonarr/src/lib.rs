use application::ports::GetSonarrShow;
use application::ports::SonarrEpisode;
use application::ports::SonarrEpisodeFile;
use application::ports::SonarrError;
use application::ports::SonarrShow;
use jiff::Timestamp;
use jiff::civil::Date;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use tracing::Instrument;
use tracing::field;
use url::Url;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct SonarrSettings {
    pub url: Option<Url>,
    pub api_key: Option<String>,
    pub webhook_password: Option<String>,
}

#[ohno::error]
#[display("missing setting: {name}")]
pub struct MissingSetting {
    name: &'static str,
}

impl SonarrSettings {
    pub fn webhook_password(&self) -> Result<String, MissingSetting> {
        self.webhook_password
            .clone()
            .ok_or_else(|| MissingSetting::new("SONARR__WEBHOOK_PASSWORD"))
    }

    pub fn get_show(&self) -> Result<GetSonarrShow, MissingSetting> {
        let api = Api {
            client: reqwest::Client::new(),
            url: self
                .url
                .clone()
                .ok_or_else(|| MissingSetting::new("SONARR__URL"))?,
            api_key: self
                .api_key
                .clone()
                .ok_or_else(|| MissingSetting::new("SONARR__API_KEY"))?,
        };
        Ok(GetSonarrShow::new(move |sonarr_id| {
            let api = api.clone();
            Box::pin(async move { api.show(sonarr_id).await.map_err(SonarrError::caused_by) })
        }))
    }
}

#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    url: Url,
    api_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Series {
    tvdb_id: i64,
    title: String,
    year: i64,
    genres: Vec<String>,
    overview: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Episode {
    tvdb_id: i64,
    episode_file_id: i64,
    season_number: i64,
    episode_number: i64,
    title: String,
    air_date: Option<Date>,
    overview: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeFile {
    id: i64,
    path: String,
    date_added: Timestamp,
}

impl Api {
    async fn show(&self, sonarr_id: i64) -> Result<Option<SonarrShow>, reqwest::Error> {
        let id = sonarr_id.to_string();
        let series = match self
            .get::<Series>("/api/v3/series/{id}", &["series", &id], None)
            .await
        {
            Err(error) if error.status() == Some(StatusCode::NOT_FOUND) => return Ok(None),
            series => series?,
        };
        let query = Some(("seriesId", id.as_str()));
        let episodes = self
            .get::<Vec<Episode>>("/api/v3/episode", &["episode"], query)
            .await?;
        let files = self
            .get::<Vec<EpisodeFile>>("/api/v3/episodefile", &["episodefile"], query)
            .await?
            .into_iter()
            .map(|x| (x.id, x))
            .collect::<HashMap<_, _>>();
        Ok(Some(SonarrShow {
            tvdb_id: series.tvdb_id,
            title: series.title,
            year: (series.year != 0).then_some(series.year),
            genres: series.genres,
            overview: series.overview,
            episodes: episodes
                .into_iter()
                .map(|x| SonarrEpisode {
                    tvdb_id: x.tvdb_id,
                    season_number: x.season_number,
                    episode_number: x.episode_number,
                    title: x.title,
                    air_date: x.air_date,
                    overview: x.overview,
                    episode_file: files.get(&x.episode_file_id).map(|x| SonarrEpisodeFile {
                        path: x.path.clone(),
                        date_added: x.date_added,
                    }),
                })
                .collect(),
        }))
    }

    async fn get<T: DeserializeOwned>(
        &self,
        template: &'static str,
        path: &[&str],
        query: Option<(&str, &str)>,
    ) -> Result<T, reqwest::Error> {
        let mut url = self.url.clone();
        url.path_segments_mut()
            .expect("SONARR__URL is an http or https URL")
            .pop_if_empty()
            .extend(["api", "v3"])
            .extend(path);
        if let Some((name, value)) = query {
            url.query_pairs_mut().append_pair(name, value);
        }
        let span = tracing::info_span!(
            "sonarr_request",
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
