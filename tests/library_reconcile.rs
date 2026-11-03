#[path = "../test/library.rs"]
mod library_test;

use application::library::sync_status::Output as SyncStatus;
use application::library::sync_status::SourceStatus;
use domain::Source;
use jiff::SignedDuration;
use library_test::LibraryServer;
use library_test::MediaFolder;
use library_test::StubRadarr;
use library_test::StubSonarr;
use library_test::eventually;
use library_test::selfless_test::settings;
use reqwest::StatusCode;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;

const ALL_MOVIES: [i64; 4] = [42, 43, 44, 45];
const SEVERANCE: i64 = 7;
const ANDOR: i64 = 8;

struct Fixture {
    database: TestDatabase,
    server: LibraryServer,
    arrs: Arrs,
}

struct Arrs {
    radarr: StubRadarr,
    sonarr: StubSonarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start(movies: &[i64], shows: &[i64]) -> Self {
        Self::start_with_interval(movies, shows, Duration::from_secs(3600)).await
    }

    async fn start_with_interval(movies: &[i64], shows: &[i64], interval: Duration) -> Self {
        let database = TestDatabase::create().await;
        let arrs = Arrs {
            radarr: StubRadarr::start().await,
            sonarr: StubSonarr::start().await,
            media: MediaFolder::new(),
        };
        arrs.radarr_lists(movies).await;
        arrs.sonarr_lists(shows).await;
        let mut settings = settings(database.url());
        settings.radarr.url = Some(arrs.radarr.url());
        settings.sonarr.url = Some(arrs.sonarr.url());
        settings.library_sync.reconcile_interval = interval;
        Self {
            server: LibraryServer::start_with(settings, FakeClock::new()).await,
            database,
            arrs,
        }
    }

    async fn status(&self) -> SyncStatus {
        application::library::sync_status::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn reconciled(&self, source: Source) -> SourceStatus {
        let started_at = self.server.clock.now();
        let source_status = async || {
            let status = self.status().await;
            match source {
                Source::Radarr => status.radarr,
                Source::Sonarr => status.sonarr,
            }
        };
        eventually(async || {
            source_status()
                .await
                .last_reconcile
                .is_some_and(|x| x.started_at == started_at)
                .then_some(())
        })
        .await;
        source_status().await
    }

    async fn sync_now(&self, source: Source) -> SourceStatus {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::library::sync_now::call(&self.server.state).unwrap();
        self.reconciled(source).await
    }

    async fn movie_titles(&self) -> Vec<String> {
        application::library::list_movies::call(&self.server.state)
            .await
            .unwrap()
            .movies
            .into_iter()
            .map(|x| x.title)
            .collect()
    }

    async fn show_titles(&self) -> Vec<String> {
        application::library::list_shows::call(&self.server.state)
            .await
            .unwrap()
            .shows
            .into_iter()
            .map(|x| x.title)
            .collect()
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

impl Arrs {
    async fn radarr_lists(&self, ids: &[i64]) {
        let movies = self.listed_movies(ids);
        self.radarr
            .serve_movies(StatusCode::OK, &Value::Array(movies).to_string())
            .await;
    }

    fn listed_movies(&self, ids: &[i64]) -> Vec<Value> {
        let movies: Vec<Value> =
            serde_json::from_str(&self.media.fixture("radarr/api/movies.json")).unwrap();
        let movies = movies
            .into_iter()
            .filter(|x| ids.contains(&x["id"].as_i64().unwrap()))
            .collect::<Vec<_>>();
        for movie in &movies {
            self.media
                .create_absolute(movie["movieFile"]["path"].as_str().unwrap());
        }
        movies
    }

    async fn sonarr_lists(&self, ids: &[i64]) {
        let series = ids
            .iter()
            .map(|x| {
                self.media
                    .fixture(&format!("sonarr/api/series{}.json", suffix(*x)))
            })
            .collect::<Vec<_>>();
        self.sonarr
            .serve_series_list(&format!("[{}]", series.join(",")))
            .await;
        for id in ids {
            let files = self
                .media
                .fixture(&format!("sonarr/api/episode-files{}.json", suffix(*id)));
            for file in serde_json::from_str::<Vec<Value>>(&files).unwrap() {
                self.media.create_absolute(file["path"].as_str().unwrap());
            }
            let episodes_json = self
                .media
                .fixture(&format!("sonarr/api/episodes{}.json", suffix(*id)));
            self.sonarr
                .serve_episodes(*id, &episodes_json, &files, Duration::ZERO)
                .await;
        }
    }
}

fn suffix(sonarr_id: i64) -> &'static str {
    match sonarr_id {
        SEVERANCE => "",
        ANDOR => "-andor",
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn movies_and_shows_are_sorted_by_their_sort_title() {
    let fixture = Fixture::start(&[], &[]).await;
    fixture.reconciled(Source::Radarr).await;
    let movies = fixture.arrs.listed_movies(&ALL_MOVIES);
    let arrival = movies[0]["movieFile"]["path"].as_str().unwrap();
    std::fs::write(
        Path::new(arrival).with_extension("nfo"),
        "<movie><title>The Arrival</title><sorttitle>Arrival</sorttitle></movie>",
    )
    .unwrap();
    fixture
        .arrs
        .radarr
        .serve_movies(StatusCode::OK, &Value::Array(movies).to_string())
        .await;
    fixture.arrs.sonarr_lists(&[SEVERANCE, ANDOR]).await;
    let andor: Value =
        serde_json::from_str(&fixture.arrs.media.fixture("sonarr/api/series-andor.json")).unwrap();
    std::fs::write(
        Path::new(andor["path"].as_str().unwrap()).join("tvshow.nfo"),
        "<tvshow><title>Andor</title><sorttitle>Star Wars: Andor</sorttitle></tvshow>",
    )
    .unwrap();

    fixture.sync_now(Source::Radarr).await;
    fixture.reconciled(Source::Sonarr).await;

    assert_eq!(
        fixture.movie_titles().await,
        ["The Arrival", "Blade Runner 2049", "Dune", "Sicario"]
    );
    assert_eq!(fixture.show_titles().await, ["Severance", "Andor"]);
    fixture.stop().await;
}
