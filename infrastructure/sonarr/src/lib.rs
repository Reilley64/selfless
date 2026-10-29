use application::ports::CheckSonarrKodiMetadata;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::ListSonarrSeries;
use application::ports::SonarrEpisode;
use application::ports::SonarrEpisodeFile;
use application::ports::SonarrError;
use application::ports::SonarrSeries;
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
        let api = self.api()?;
        Ok(GetSonarrShow::new(move |sonarr_id| {
            let api = api.clone();
            Box::pin(async move { api.show(sonarr_id).await.map_err(SonarrError::caused_by) })
        }))
    }

    pub fn list_series(&self) -> Result<ListSonarrSeries, MissingSetting> {
        let api = self.api()?;
        Ok(ListSonarrSeries::new(move || {
            let api = api.clone();
            Box::pin(async move {
                Ok(api
                    .get::<Vec<Series>>("/api/v3/series", &["series"], None)
                    .await
                    .map_err(SonarrError::caused_by)?
                    .into_iter()
                    .map(SonarrSeries::from)
                    .collect())
            })
        }))
    }

    pub fn get_episodes(&self) -> Result<GetSonarrEpisodes, MissingSetting> {
        let api = self.api()?;
        Ok(GetSonarrEpisodes::new(move |sonarr_id| {
            let api = api.clone();
            Box::pin(async move {
                api.episodes(sonarr_id)
                    .await
                    .map_err(SonarrError::caused_by)
            })
        }))
    }

    pub fn check_kodi_metadata(&self) -> Result<CheckSonarrKodiMetadata, MissingSetting> {
        let api = self.api()?;
        Ok(CheckSonarrKodiMetadata::new(move || {
            let api = api.clone();
            Box::pin(async move {
                Ok(api
                    .get::<Vec<MetadataConsumer>>("/api/v3/metadata", &["metadata"], None)
                    .await
                    .map_err(SonarrError::caused_by)?
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
                .ok_or_else(|| MissingSetting::new("SONARR__URL"))?,
            api_key: self
                .api_key
                .clone()
                .ok_or_else(|| MissingSetting::new("SONARR__API_KEY"))?,
        })
    }
}

#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    url: Url,
    api_key: String,
}

#[derive(Deserialize)]
struct MetadataConsumer {
    implementation: String,
    enable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Series {
    id: i64,
    tvdb_id: i64,
    path: String,
    title: String,
    year: i64,
    genres: Vec<String>,
    overview: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Episode {
    id: i64,
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
        let series = match self
            .get::<Series>(
                "/api/v3/series/{id}",
                &["series", &sonarr_id.to_string()],
                None,
            )
            .await
        {
            Err(error) if error.status() == Some(StatusCode::NOT_FOUND) => return Ok(None),
            series => series?,
        };
        Ok(Some(SonarrShow {
            series: series.into(),
            episodes: self.episodes(sonarr_id).await?,
        }))
    }

    async fn episodes(&self, sonarr_id: i64) -> Result<Vec<SonarrEpisode>, reqwest::Error> {
        let id = sonarr_id.to_string();
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
        Ok(episodes
            .into_iter()
            .map(|x| SonarrEpisode {
                sonarr_id: x.id,
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
            .collect())
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

impl From<Series> for SonarrSeries {
    fn from(series: Series) -> Self {
        SonarrSeries {
            sonarr_id: series.id,
            tvdb_id: series.tvdb_id,
            path: series.path,
            title: series.title,
            year: (series.year != 0).then_some(series.year),
            genres: series.genres,
            overview: series.overview,
        }
    }
}
