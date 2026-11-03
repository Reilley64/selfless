#[path = "../test/persistence.rs"]
mod persistence_test;
#[path = "../test/selfless.rs"]
pub mod selfless_test;

use application::ports::persistence::DbScope;
use domain::ReconcileResult;
use domain::Source;
use domain::SourceState;
use selfless_test::MediaFolder;
use selfless_test::StubRadarr;
use selfless_test::StubSonarr;
use selfless_test::eventually;
use serde_json::Value;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use test_fixtures::TestDatabase;

const SEVERANCE: i64 = 7;
const ANDOR: i64 = 8;

fn create_files(media: &MediaFolder, paths: impl IntoIterator<Item = Value>) {
    for path in paths {
        media.create(path.as_str().unwrap().strip_prefix("/mnt/media/").unwrap());
    }
}

#[tokio::test]
async fn the_first_reconcile_at_startup_imports_every_movie_and_show() {
    let database = TestDatabase::create().await;
    let media = MediaFolder::default();
    let radarr = StubRadarr::start().await;
    let movies: Vec<Value> =
        serde_json::from_str(&selfless_test::fixture("radarr/api/movies.json")).unwrap();
    create_files(
        &media,
        movies.iter().map(|x| x["movieFile"]["path"].clone()),
    );
    radarr
        .serve_movies(&media.fixture("radarr/api/movies.json"))
        .await;
    let sonarr = StubSonarr::start().await;
    sonarr
        .serve_series_list(&format!(
            "[{},{}]",
            media.fixture("sonarr/api/series.json"),
            media.fixture("sonarr/api/series-andor.json")
        ))
        .await;
    for (id, suffix) in [(SEVERANCE, ""), (ANDOR, "-andor")] {
        let episode_files = format!("sonarr/api/episode-files{suffix}.json");
        let files: Vec<Value> =
            serde_json::from_str(&selfless_test::fixture(&episode_files)).unwrap();
        create_files(&media, files.into_iter().map(|x| x["path"].clone()));
        sonarr
            .serve_episodes(
                id,
                &media.fixture(&format!("sonarr/api/episodes{suffix}.json")),
                &media.fixture(&episode_files),
            )
            .await;
    }
    let mut settings = selfless_test::settings(database.url());
    settings.radarr.url = Some(radarr.url());
    settings.sonarr.url = Some(sonarr.url());
    let server = selfless_test::start(settings, &FakeClock::new(), &FakeMailer::default()).await;
    let mut db_context = persistence_test::db_context(&database).await;

    let results = eventually(async || {
        let states = db_context
            .source_states()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        let result = |source| {
            states
                .iter()
                .find(|x: &&SourceState| x.source == source)
                .and_then(|x| x.last_reconcile)
                .map(|x| x.result())
        };
        Some((result(Source::Radarr)?, result(Source::Sonarr)?))
    })
    .await;
    let mut movies = db_context
        .movies()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.title)
        .collect::<Vec<_>>();
    movies.sort();
    let mut shows = db_context
        .shows()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.title)
        .collect::<Vec<_>>();
    shows.sort();

    assert_eq!(
        results,
        (ReconcileResult::Succeeded, ReconcileResult::Succeeded)
    );
    assert_eq!(movies, ["Arrival", "Blade Runner 2049", "Dune", "Sicario"]);
    assert_eq!(shows, ["Andor", "Severance"]);
    drop(db_context);
    drop(server);
    database.drop().await;
}
