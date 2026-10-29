mod support;

use application::sync_status::HeldRemoval;
use application::sync_status::ReconcileProgress;
use application::sync_status::SourceStatus;
use application::sync_status::SyncStatus;
use application::sync_status::VisibleCounts;
use domain::ReconcileResult;
use domain::Source;
use jiff::SignedDuration;
use reqwest::StatusCode;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;
use support::FakeClock;
use support::MediaFolder;
use support::StubRadarr;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::settings;

const ALL_MOVIES: [i64; 4] = [42, 43, 44, 45];
const SEVERANCE: i64 = 7;
const ANDOR: i64 = 8;

struct Fixture {
    database: TestDatabase,
    server: TestServer,
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
            server: TestServer::start_with(settings, FakeClock::new()).await,
            database,
            arrs,
        }
    }

    async fn status(&self) -> SyncStatus {
        application::sync_status::call(&self.server.state, None, 1)
            .await
            .unwrap()
    }

    async fn reconciled(&self, source: Source) -> SourceStatus {
        let started_at = self.server.clock.now();
        eventually(async || {
            let status = self.status().await;
            let status = match source {
                Source::Radarr => status.radarr,
                Source::Sonarr => status.sonarr,
            };
            status
                .last_reconcile
                .as_ref()
                .is_some_and(|x| x.started_at == started_at)
                .then_some(status)
        })
        .await
    }

    async fn sync_now(&self, source: Source) -> SourceStatus {
        self.server.clock.advance(SignedDuration::from_mins(1));
        application::sync_now::call(&self.server.state);
        self.reconciled(source).await
    }

    async fn confirm_held_removal(&self, source: Source) {
        application::confirm_held_removal::call(&self.server.state, source)
            .await
            .unwrap();
    }

    async fn movie_titles(&self) -> Vec<String> {
        application::list_movies::call(&self.server.state)
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.title)
            .collect()
    }

    async fn show_titles(&self) -> Vec<String> {
        application::list_shows::call(&self.server.state)
            .await
            .unwrap()
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
        self.sonarr_lists_with(ids, |_| None).await;
    }

    async fn sonarr_lists_with(&self, ids: &[i64], episodes: impl Fn(i64) -> Option<Episodes>) {
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
            match episodes(*id) {
                None => {
                    self.sonarr
                        .serve_episodes(*id, &episodes_json, &files, Duration::ZERO)
                        .await
                }
                Some(Episodes::Delayed(delay)) => {
                    self.sonarr
                        .serve_episodes(*id, &episodes_json, &files, delay)
                        .await
                }
                Some(Episodes::Failing) => self.sonarr.fail_episodes(*id).await,
            }
        }
    }
}

enum Episodes {
    Delayed(Duration),
    Failing,
}

fn suffix(sonarr_id: i64) -> &'static str {
    match sonarr_id {
        SEVERANCE => "",
        ANDOR => "-andor",
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn the_first_reconcile_at_startup_imports_every_movie_and_show() {
    let fixture = Fixture::start(&ALL_MOVIES, &[SEVERANCE, ANDOR]).await;

    let radarr = fixture.reconciled(Source::Radarr).await;
    let sonarr = fixture.reconciled(Source::Sonarr).await;

    assert_eq!(
        radarr.last_reconcile.unwrap().result,
        ReconcileResult::Succeeded
    );
    assert_eq!(radarr.visible, VisibleCounts::Movies(4));
    assert_eq!(
        sonarr.last_reconcile.unwrap().result,
        ReconcileResult::Succeeded
    );
    assert_eq!(
        sonarr.visible,
        VisibleCounts::Shows {
            shows: 2,
            episodes: 7
        }
    );
    assert_eq!(
        fixture.movie_titles().await,
        ["Arrival", "Blade Runner 2049", "Dune", "Sicario"]
    );
    assert_eq!(fixture.show_titles().await, ["Andor", "Severance"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_adds_new_movies_and_removes_movies_radarr_no_longer_lists() {
    let fixture = Fixture::start(&[42, 43, 44], &[]).await;
    fixture.reconciled(Source::Radarr).await;

    fixture.arrs.radarr_lists(&[42, 43, 45]).await;
    let status = fixture.sync_now(Source::Radarr).await;

    assert_eq!(
        status.last_reconcile.unwrap().result,
        ReconcileResult::Succeeded
    );
    assert_eq!(fixture.movie_titles().await, ["Arrival", "Dune", "Sicario"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_re_syncs_a_movie_whose_radarr_data_changed() {
    let fixture = Fixture::start(&[42], &[]).await;
    fixture.reconciled(Source::Radarr).await;

    let mut movies = fixture.arrs.listed_movies(&[42]);
    movies[0]["title"] = "Arrival (Extended)".into();
    fixture
        .arrs
        .radarr
        .serve_movies(StatusCode::OK, &Value::Array(movies).to_string())
        .await;
    fixture.sync_now(Source::Radarr).await;

    assert_eq!(fixture.movie_titles().await, ["Arrival (Extended)"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_listing_call_removes_nothing() {
    let fixture = Fixture::start(&[42, 43], &[]).await;
    fixture.reconciled(Source::Radarr).await;

    fixture
        .arrs
        .radarr
        .serve_movies(StatusCode::INTERNAL_SERVER_ERROR, "{}")
        .await;
    let status = fixture.sync_now(Source::Radarr).await;

    assert_eq!(
        status.last_reconcile.unwrap().result,
        ReconcileResult::Failed
    );
    assert_eq!(fixture.movie_titles().await, ["Arrival", "Dune"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_metadata_call_removes_nothing_but_still_adds_movies() {
    let fixture = Fixture::start(&[42, 43], &[]).await;
    fixture.reconciled(Source::Radarr).await;

    fixture.arrs.radarr_lists(&[42, 44]).await;
    fixture.arrs.radarr.fail_metadata().await;
    let status = fixture.sync_now(Source::Radarr).await;

    assert_eq!(
        status.last_reconcile.unwrap().result,
        ReconcileResult::Failed
    );
    assert_eq!(
        fixture.movie_titles().await,
        ["Arrival", "Blade Runner 2049", "Dune"]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_failed_episodes_call_removes_no_show_until_every_call_succeeds() {
    let fixture = Fixture::start(&[], &[SEVERANCE, ANDOR]).await;
    fixture.reconciled(Source::Sonarr).await;

    fixture
        .arrs
        .sonarr_lists_with(&[SEVERANCE], |_| Some(Episodes::Failing))
        .await;
    let failed = fixture.sync_now(Source::Sonarr).await;
    assert_eq!(
        failed.last_reconcile.unwrap().result,
        ReconcileResult::Failed
    );
    assert_eq!(fixture.show_titles().await, ["Andor", "Severance"]);

    fixture.arrs.sonarr_lists(&[SEVERANCE]).await;
    let succeeded = fixture.sync_now(Source::Sonarr).await;
    assert_eq!(
        succeeded.last_reconcile.unwrap().result,
        ReconcileResult::Succeeded
    );
    assert_eq!(fixture.show_titles().await, ["Severance"]);
    fixture.stop().await;
}

#[tokio::test]
async fn a_removal_of_more_than_half_the_movies_is_held_until_an_admin_confirms_it() {
    let fixture = Fixture::start(&ALL_MOVIES, &[]).await;
    fixture.reconciled(Source::Radarr).await;

    fixture.arrs.radarr_lists(&[42]).await;
    let status = fixture.sync_now(Source::Radarr).await;
    assert_eq!(status.held_removal, Some(HeldRemoval { count: 3 }));
    assert_eq!(fixture.movie_titles().await.len(), 4);

    fixture.confirm_held_removal(Source::Radarr).await;

    assert_eq!(fixture.movie_titles().await, ["Arrival"]);
    assert_eq!(fixture.status().await.radarr.held_removal, None);
    fixture.stop().await;
}

#[tokio::test]
async fn confirming_a_held_removal_keeps_the_movies_radarr_lists_again() {
    let fixture = Fixture::start(&ALL_MOVIES, &[]).await;
    fixture.reconciled(Source::Radarr).await;
    fixture.arrs.radarr_lists(&[42]).await;
    let held = fixture.sync_now(Source::Radarr).await;
    assert_eq!(held.held_removal, Some(HeldRemoval { count: 3 }));

    fixture.arrs.radarr_lists(&[42, 43]).await;
    fixture.confirm_held_removal(Source::Radarr).await;

    assert_eq!(fixture.movie_titles().await, ["Arrival", "Dune"]);
    assert_eq!(fixture.status().await.radarr.held_removal, None);
    fixture.stop().await;
}

#[tokio::test]
async fn a_radarr_that_lists_as_many_other_movies_holds_the_removal_of_every_stored_movie() {
    let fixture = Fixture::start(&ALL_MOVIES, &[]).await;
    fixture.reconciled(Source::Radarr).await;

    let other_movies = fixture
        .arrs
        .listed_movies(&ALL_MOVIES)
        .into_iter()
        .map(|mut movie| {
            movie["id"] = (movie["id"].as_i64().unwrap() + 100).into();
            movie["tmdbId"] = (movie["tmdbId"].as_i64().unwrap() + 1_000_000).into();
            movie["title"] = format!("Other {}", movie["title"].as_str().unwrap()).into();
            let path = movie["movieFile"]["path"]
                .as_str()
                .unwrap()
                .replace("/Movies/", "/Other Movies/");
            fixture.arrs.media.create_absolute(&path);
            movie["movieFile"]["path"] = path.into();
            movie
        })
        .collect();
    fixture
        .arrs
        .radarr
        .serve_movies(StatusCode::OK, &Value::Array(other_movies).to_string())
        .await;
    let status = fixture.sync_now(Source::Radarr).await;

    assert_eq!(status.held_removal, Some(HeldRemoval { count: 4 }));
    assert_eq!(
        fixture.movie_titles().await,
        [
            "Arrival",
            "Blade Runner 2049",
            "Dune",
            "Other Arrival",
            "Other Blade Runner 2049",
            "Other Dune",
            "Other Sicario",
            "Sicario"
        ]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_later_reconcile_with_a_smaller_removal_replaces_the_held_one() {
    let fixture = Fixture::start(&ALL_MOVIES, &[]).await;
    fixture.reconciled(Source::Radarr).await;
    fixture.arrs.radarr_lists(&[]).await;
    let held = fixture.sync_now(Source::Radarr).await;
    assert_eq!(held.held_removal, Some(HeldRemoval { count: 4 }));

    fixture.arrs.radarr_lists(&[42]).await;
    let replaced = fixture.sync_now(Source::Radarr).await;
    assert_eq!(replaced.held_removal, Some(HeldRemoval { count: 3 }));
    fixture.confirm_held_removal(Source::Radarr).await;

    assert_eq!(fixture.movie_titles().await, ["Arrival"]);
    fixture.stop().await;
}

#[tokio::test]
async fn the_sync_status_shows_imported_n_of_m_while_a_reconcile_runs() {
    let fixture = Fixture::start(&[], &[]).await;
    fixture.reconciled(Source::Sonarr).await;
    fixture
        .arrs
        .sonarr_lists_with(&[SEVERANCE, ANDOR], |x| {
            (x == ANDOR).then_some(Episodes::Delayed(Duration::from_secs(2)))
        })
        .await;

    fixture.server.clock.advance(SignedDuration::from_mins(1));
    application::sync_now::call(&fixture.server.state);

    eventually(async || {
        (fixture.status().await.sonarr.reconcile_progress
            == Some(ReconcileProgress {
                imported: 1,
                total: Some(2),
            }))
        .then_some(())
    })
    .await;
    assert_eq!(fixture.show_titles().await, ["Severance"]);
    let finished = fixture.reconciled(Source::Sonarr).await;
    assert_eq!(finished.reconcile_progress, None);
    assert_eq!(fixture.show_titles().await, ["Andor", "Severance"]);
    fixture.stop().await;
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

#[tokio::test]
async fn a_reconcile_runs_again_after_the_reconcile_interval() {
    let fixture = Fixture::start_with_interval(&[42], &[], Duration::from_millis(200)).await;
    eventually(async || (fixture.movie_titles().await == ["Arrival"]).then_some(())).await;

    fixture.arrs.radarr_lists(&[42, 43]).await;

    eventually(async || (fixture.movie_titles().await == ["Arrival", "Dune"]).then_some(())).await;
    fixture.stop().await;
}
