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
use servarr::Api;
use servarr::MissingSetting;
use servarr::setting;
use std::collections::HashMap;
use url::Url;

#[derive(Clone, Default, Deserialize)]
#[serde(default)]
pub struct SonarrSettings {
    pub url: Option<Url>,
    pub api_key: Option<String>,
    pub webhook_password: Option<String>,
}

impl SonarrSettings {
    pub fn webhook_password(&self) -> Result<String, MissingSetting> {
        setting(&self.webhook_password, "SONARR__WEBHOOK_PASSWORD")
    }

    pub fn get_show(&self) -> Result<GetSonarrShow, MissingSetting> {
        let api = self.api()?;
        Ok(GetSonarrShow::new(move |sonarr_id| {
            let api = api.clone();
            Box::pin(async move { show(&api, sonarr_id).await.map_err(SonarrError::caused_by) })
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
                episodes(&api, sonarr_id)
                    .await
                    .map_err(SonarrError::caused_by)
            })
        }))
    }

    pub fn check_kodi_metadata(&self) -> Result<CheckSonarrKodiMetadata, MissingSetting> {
        let api = self.api()?;
        Ok(CheckSonarrKodiMetadata::new(move || {
            let api = api.clone();
            Box::pin(async move { api.kodi_metadata().await.map_err(SonarrError::caused_by) })
        }))
    }

    fn api(&self) -> Result<Api, MissingSetting> {
        Ok(Api::new(
            setting(&self.url, "SONARR__URL")?,
            setting(&self.api_key, "SONARR__API_KEY")?,
        ))
    }
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

async fn show(api: &Api, sonarr_id: i64) -> Result<Option<SonarrShow>, reqwest::Error> {
    let series = match api
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
        episodes: episodes(api, sonarr_id).await?,
    }))
}

async fn episodes(api: &Api, sonarr_id: i64) -> Result<Vec<SonarrEpisode>, reqwest::Error> {
    let id = sonarr_id.to_string();
    let query = Some(("seriesId", id.as_str()));
    let episodes = api
        .get::<Vec<Episode>>("/api/v3/episode", &["episode"], query)
        .await?;
    let files = api
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
