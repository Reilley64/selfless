mod webhook;

pub use webhook::read_webhook;

use application::ports::CheckSonarrKodiMetadata;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::ListSonarrShows;
use application::ports::SonarrEpisode;
use application::ports::SonarrEpisodeFile;
use application::ports::SonarrError;
use application::ports::SonarrShow;
use application::ports::SonarrShowListing;
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

    pub fn list_shows(&self) -> Result<ListSonarrShows, MissingSetting> {
        let api = self.api()?;
        Ok(ListSonarrShows::new(move || {
            let api = api.clone();
            Box::pin(async move {
                Ok(api
                    .get::<Vec<Series>>("/api/v3/series", &["series"], None)
                    .await
                    .map_err(SonarrError::caused_by)?
                    .into_iter()
                    .map(SonarrShowListing::from)
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
        listing: series.into(),
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

impl From<Series> for SonarrShowListing {
    fn from(series: Series) -> Self {
        SonarrShowListing {
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

#[cfg(test)]
mod tests {
    use super::SonarrSettings;
    use jiff::Timestamp;
    use jiff::civil::date;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::query_param;

    const SERIES: &str = include_str!("../../../test/fixtures/sonarr/api/series.json");
    const SERIES_ANDOR: &str = include_str!("../../../test/fixtures/sonarr/api/series-andor.json");
    const EPISODES: &str = include_str!("../../../test/fixtures/sonarr/api/episodes.json");
    const EPISODE_FILES: &str =
        include_str!("../../../test/fixtures/sonarr/api/episode-files.json");

    struct Sonarr(MockServer);

    impl Sonarr {
        async fn start() -> Self {
            Self(MockServer::start().await)
        }

        async fn serve(&self, url_path: &str, series_id: Option<i64>, status: u16, body: &str) {
            let mut mock = Mock::given(method("GET")).and(path(url_path));
            if let Some(series_id) = series_id {
                mock = mock.and(query_param("seriesId", series_id.to_string()));
            }
            mock.respond_with(
                ResponseTemplate::new(status).set_body_raw(body.to_owned(), "application/json"),
            )
            .mount(&self.0)
            .await;
        }

        async fn serve_show(&self, series: &str) {
            self.serve("/api/v3/series/7", None, 200, series).await;
            self.serve("/api/v3/episode", Some(7), 200, EPISODES).await;
            self.serve("/api/v3/episodefile", Some(7), 200, EPISODE_FILES)
                .await;
        }

        fn settings(&self) -> SonarrSettings {
            SonarrSettings {
                url: Some(self.0.uri().parse().unwrap()),
                api_key: Some("sonarr-api-key".to_owned()),
                webhook_password: None,
            }
        }
    }

    #[tokio::test]
    async fn get_show_joins_the_series_its_episodes_and_their_files() {
        let sonarr = Sonarr::start().await;
        sonarr.serve_show(SERIES).await;

        let show = sonarr
            .settings()
            .get_show()
            .unwrap()
            .call(7)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(show.listing.sonarr_id, 7);
        assert_eq!(show.listing.tvdb_id, 371980);
        assert_eq!(show.listing.path, "/mnt/media/TV/Severance");
        assert_eq!(show.listing.title, "Severance");
        assert_eq!(show.listing.year, Some(2022));
        assert_eq!(
            show.listing.genres,
            ["Drama", "Mystery", "Science Fiction", "Thriller"]
        );
        assert!(
            show.listing
                .overview
                .as_deref()
                .unwrap()
                .starts_with("Mark leads a team of office workers")
        );
        let two_episodes = "/mnt/media/TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p.mkv";
        assert_eq!(
            show.episodes
                .iter()
                .map(|x| (
                    x.sonarr_id,
                    x.season_number,
                    x.episode_number,
                    x.title.as_str(),
                    x.episode_file.as_ref().map(|x| x.path.as_str()),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    1001,
                    0,
                    1,
                    "Inside the Episode",
                    Some(
                        "/mnt/media/TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p.mkv"
                    ),
                ),
                (
                    1011,
                    1,
                    1,
                    "Good News About Hell",
                    Some(
                        "/mnt/media/TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv"
                    ),
                ),
                (1012, 1, 2, "Half Loop", Some(two_episodes)),
                (1013, 1, 3, "In Perpetuity", Some(two_episodes)),
                (
                    1021,
                    2,
                    1,
                    "Hello, Ms. Cobel",
                    Some(
                        "/mnt/media/TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv"
                    ),
                ),
                (1022, 2, 2, "Goodbye, Mrs. Selvig", None),
                (1031, 3, 1, "TBA", None),
            ]
        );
        let first = &show.episodes[1];
        assert_eq!(first.tvdb_id, 8486245);
        assert_eq!(first.air_date, Some(date(2022, 2, 18)));
        assert_eq!(
            first.overview.as_deref(),
            Some(
                "Mark leads a team of office workers whose memories have been surgically \
                 divided between their work and personal lives."
            )
        );
        assert_eq!(
            first.episode_file.as_ref().unwrap().date_added,
            "2022-03-01T09:30:00Z".parse::<Timestamp>().unwrap()
        );
        let unaired = &show.episodes[6];
        assert_eq!(unaired.air_date, None);
        assert_eq!(unaired.overview, None);
    }

    #[tokio::test]
    async fn a_show_sonarr_does_not_have_is_none() {
        let sonarr = Sonarr::start().await;
        sonarr
            .serve(
                "/api/v3/series/7",
                None,
                404,
                include_str!("../../../test/fixtures/sonarr/api/series-not-found.json"),
            )
            .await;

        let show = sonarr.settings().get_show().unwrap().call(7).await.unwrap();

        assert!(show.is_none());
    }

    #[tokio::test]
    async fn a_show_with_year_zero_has_no_year() {
        let sonarr = Sonarr::start().await;
        sonarr
            .serve_show(&SERIES.replace(r#""year": 2022"#, r#""year": 0"#))
            .await;

        let show = sonarr
            .settings()
            .get_show()
            .unwrap()
            .call(7)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(show.listing.year, None);
    }

    #[tokio::test]
    async fn list_shows_maps_every_listed_show() {
        let sonarr = Sonarr::start().await;
        sonarr
            .serve(
                "/api/v3/series",
                None,
                200,
                &format!("[{SERIES},{SERIES_ANDOR}]"),
            )
            .await;

        let shows = sonarr
            .settings()
            .list_shows()
            .unwrap()
            .call()
            .await
            .unwrap();

        assert_eq!(
            shows
                .iter()
                .map(|x| (
                    x.sonarr_id,
                    x.tvdb_id,
                    x.title.as_str(),
                    x.path.as_str(),
                    x.year
                ))
                .collect::<Vec<_>>(),
            [
                (
                    7,
                    371980,
                    "Severance",
                    "/mnt/media/TV/Severance",
                    Some(2022)
                ),
                (8, 393189, "Andor", "/mnt/media/TV/Andor", Some(2022)),
            ]
        );
    }

    #[tokio::test]
    async fn get_episodes_maps_the_episodes_and_their_files() {
        let sonarr = Sonarr::start().await;
        sonarr
            .serve(
                "/api/v3/episode",
                Some(8),
                200,
                include_str!("../../../test/fixtures/sonarr/api/episodes-andor.json"),
            )
            .await;
        sonarr
            .serve(
                "/api/v3/episodefile",
                Some(8),
                200,
                include_str!("../../../test/fixtures/sonarr/api/episode-files-andor.json"),
            )
            .await;

        let episodes = sonarr
            .settings()
            .get_episodes()
            .unwrap()
            .call(8)
            .await
            .unwrap();

        assert_eq!(
            episodes
                .iter()
                .map(|x| (
                    x.sonarr_id,
                    x.title.as_str(),
                    x.episode_file.as_ref().map(|x| x.path.as_str()),
                ))
                .collect::<Vec<_>>(),
            [
                (
                    1101,
                    "Kassa",
                    Some("/mnt/media/TV/Andor/Season 01/Andor - S01E01 - Kassa WEBDL-1080p.mkv"),
                ),
                (
                    1102,
                    "That Would Be Me",
                    Some(
                        "/mnt/media/TV/Andor/Season 01/Andor - S01E02 - That Would Be Me WEBDL-1080p.mkv"
                    ),
                ),
            ]
        );
    }
}
