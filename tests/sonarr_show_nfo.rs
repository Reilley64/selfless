mod support;

use application::list_episodes::Episode;
use application::list_movies::Actor;
use application::list_shows::Show;
use application::sync_status::Problem;
use application::sync_status::ProblemItem;
use domain::ArtworkKind;
use domain::SyncProblemKind;
use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::civil::date;
use serde_json::Value;
use std::time::Duration;
use support::MediaFolder;
use support::StubSonarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;
use uuid::Uuid;

struct SonarrSeries {
    sonarr_id: i64,
    series: &'static str,
    episodes: &'static str,
    episode_files: &'static str,
}

const ANDOR: SonarrSeries = SonarrSeries {
    sonarr_id: 8,
    series: "series-andor",
    episodes: "episodes-andor",
    episode_files: "episode-files-andor",
};
const SEVERANCE: SonarrSeries = SonarrSeries {
    sonarr_id: 7,
    series: "series",
    episodes: "episodes",
    episode_files: "episode-files",
};
const ANDOR_FOLDER: &str = "TV/Andor";
const KASSA: &str = "TV/Andor/Season 01/Andor - S01E01 - Kassa WEBDL-1080p";
const SEVERANCE_FOLDER: &str = "TV/Severance";
const SPECIAL: &str = "TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p";
const S01E01: &str = "TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p";
const S01E02_E03: &str =
    "TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p";
const S02E01: &str = "TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p";
const IMAGE_MODIFIED_AT: &str = "2026-10-28T12:30:00Z";

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    sonarr: StubSonarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start() -> Self {
        let database = TestDatabase::create().await;
        let sonarr = StubSonarr::start().await;
        let server = TestServer::start_with_sonarr(database.url(), &sonarr).await;
        eventually(async || {
            application::sync_status::call(&server.state, None, 1)
                .await
                .unwrap()
                .sonarr
                .last_reconcile
        })
        .await;
        Self {
            database,
            server,
            sonarr,
            media: MediaFolder::new(),
        }
    }

    fn create_files(&self, series: &SonarrSeries) {
        let files: Vec<Value> = serde_json::from_str(&fixture(&format!(
            "sonarr/api/{}.json",
            series.episode_files
        )))
        .unwrap();
        for file in files {
            self.media.create(
                file["path"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("/mnt/media/")
                    .unwrap(),
            );
        }
    }

    async fn reconcile(&self, series: &SonarrSeries) -> Show {
        self.sonarr
            .serve_series_list(&format!(
                "[{}]",
                self.media
                    .fixture(&format!("sonarr/api/{}.json", series.series))
            ))
            .await;
        self.sonarr
            .serve_episodes(
                series.sonarr_id,
                &self
                    .media
                    .fixture(&format!("sonarr/api/{}.json", series.episodes)),
                &self
                    .media
                    .fixture(&format!("sonarr/api/{}.json", series.episode_files)),
                Duration::ZERO,
            )
            .await;
        self.server.clock.advance(SignedDuration::from_mins(1));
        let started_at = self.server.clock.now();
        application::sync_now::call(&self.server.state);
        eventually(async || {
            application::sync_status::call(&self.server.state, None, 1)
                .await
                .unwrap()
                .sonarr
                .last_reconcile
                .filter(|x| x.started_at == started_at)
        })
        .await;
        application::list_shows::call(&self.server.state)
            .await
            .unwrap()
            .pop()
            .unwrap()
    }

    async fn episodes(&self, show_id: Uuid, season_number: i64) -> Vec<Episode> {
        let season = application::list_seasons::call(&self.server.state, show_id)
            .await
            .unwrap()
            .into_iter()
            .find(|x| x.number == season_number)
            .unwrap();
        application::list_episodes::call(&self.server.state, season.id)
            .await
            .unwrap()
    }

    fn write_nfo(&self, path: &str, name: &str) {
        self.media
            .write(path, &fixture(&format!("sonarr/nfo/{name}.nfo")));
    }

    async fn problems(&self) -> Vec<Problem> {
        application::sync_status::call(&self.server.state, None, 1)
            .await
            .unwrap()
            .problems
    }

    async fn stop(self) {
        drop(self.server);
        self.database.drop().await;
    }
}

#[tokio::test]
async fn a_show_takes_its_metadata_from_its_tvshow_nfo() {
    let fixture = Fixture::start().await;
    fixture.create_files(&ANDOR);
    fixture.write_nfo(&format!("{ANDOR_FOLDER}/tvshow.nfo"), "tvshow");

    let show = fixture.reconcile(&ANDOR).await;

    assert_eq!(show.title, "Andor");
    assert_eq!(show.sort_title.as_deref(), Some("andor"));
    assert!(
        show.overview
            .as_deref()
            .unwrap()
            .ends_with("destined to turn him into a rebel hero.")
    );
    assert_eq!(show.certification.as_deref(), Some("TV-14"));
    assert_eq!(show.premiered, Some(date(2022, 9, 21)));
    assert_eq!(show.end_date, Some(date(2025, 5, 13)));
    assert_eq!(show.status.as_deref(), Some("Ended"));
    assert_eq!(show.network.as_deref(), Some("Disney+"));
    assert_eq!(
        show.genres,
        ["Action", "Adventure", "Drama", "Science Fiction"]
    );
    assert_eq!(show.rating, Some(8.4));
    assert_eq!(
        show.actors,
        [
            actor("Diego Luna", "Cassian Andor", 0),
            actor("Stellan Skarsgård", "Luthen Rael", 1),
            actor("Genevieve O'Reilly", "Mon Mothma", 2),
        ]
    );
    assert_eq!(fixture.problems().await, []);
    fixture.stop().await;
}

#[tokio::test]
async fn a_new_show_without_a_tvshow_nfo_uses_sonarr_until_the_next_reconcile_finds_it() {
    let fixture = Fixture::start().await;
    fixture.create_files(&ANDOR);

    let show = fixture.reconcile(&ANDOR).await;

    assert_eq!(show.title, "Andor");
    assert_eq!(show.year, Some(2022));
    assert_eq!(
        show.genres,
        ["Action", "Adventure", "Drama", "Science Fiction"]
    );
    assert_eq!(
        show.overview.as_deref(),
        Some(
            "The tale of the burgeoning rebellion against the Empire and how people and planets \
             became involved."
        )
    );
    assert_eq!(show.network, None);
    assert_eq!(show.actors, []);
    assert_eq!(fixture.problems().await, []);

    fixture.write_nfo(&format!("{ANDOR_FOLDER}/tvshow.nfo"), "tvshow");
    let show = fixture.reconcile(&ANDOR).await;

    assert_eq!(show.network.as_deref(), Some("Disney+"));
    assert_eq!(show.actors.len(), 3);
    fixture.stop().await;
}

#[tokio::test]
async fn unreadable_nfos_fall_back_to_sonarr_and_record_problems() {
    let fixture = Fixture::start().await;
    fixture.create_files(&ANDOR);
    fixture.write_nfo(&format!("{ANDOR_FOLDER}/tvshow.nfo"), "unreadable");
    fixture.write_nfo(&format!("{KASSA}.nfo"), "unreadable");

    let show = fixture.reconcile(&ANDOR).await;

    assert_eq!(show.sort_title, None);
    assert_eq!(
        show.genres,
        ["Action", "Adventure", "Drama", "Science Fiction"]
    );
    let episodes = fixture.episodes(show.id, 1).await;
    assert_eq!(episodes[0].title, "Kassa");
    assert_eq!(
        episodes[0].overview.as_deref(),
        Some("Cassian Andor looks for his missing sister.")
    );
    assert_eq!(episodes[0].rating, None);
    assert_eq!(
        fixture.problems().await,
        [
            Problem {
                kind: SyncProblemKind::UnreadableNfo,
                item: ProblemItem::Episode {
                    show: "Andor".to_owned(),
                    season: 1,
                    number: 1,
                    title: "Kassa".to_owned(),
                    id: episodes[0].id,
                },
                path: fixture.media.path(&format!("{KASSA}.nfo")),
            },
            Problem {
                kind: SyncProblemKind::UnreadableNfo,
                item: ProblemItem::Show {
                    title: "Andor".to_owned(),
                    id: show.id,
                },
                path: fixture.media.path(&format!("{ANDOR_FOLDER}/tvshow.nfo")),
            },
        ]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_nfo_with_two_episodes_fills_each_of_them() {
    let fixture = Fixture::start().await;
    fixture.create_files(&SEVERANCE);
    fixture.write_nfo(&format!("{S01E02_E03}.nfo"), "two-episodes");

    let show = fixture.reconcile(&SEVERANCE).await;

    let episodes = fixture.episodes(show.id, 1).await;
    assert_eq!(
        episodes
            .iter()
            .map(|x| (x.number, x.title.as_str(), x.air_date, x.rating))
            .collect::<Vec<_>>(),
        [
            (1, "Good News About Hell", Some(date(2022, 2, 18)), None),
            (2, "Half Loop", Some(date(2022, 2, 18)), Some(8.1)),
            (3, "In Perpetuity", Some(date(2022, 2, 25)), Some(8.3)),
        ]
    );
    assert!(
        episodes[1]
            .overview
            .as_deref()
            .unwrap()
            .ends_with("a disturbing discovery is made about Petey.")
    );
    assert!(
        episodes[2]
            .overview
            .as_deref()
            .unwrap()
            .ends_with("Mark meets someone from his past outside of work.")
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_episode_nfo_matches_by_sonarr_uniqueid_then_by_season_and_episode_number_and_keeps_sonarrs_numbers()
 {
    let fixture = Fixture::start().await;
    fixture.create_files(&SEVERANCE);
    fixture.write_nfo(&format!("{S02E01}.nfo"), "renumbered-episode");
    fixture.write_nfo(&format!("{SPECIAL}.nfo"), "special-without-sonarr-id");

    let show = fixture.reconcile(&SEVERANCE).await;

    let renumbered = fixture.episodes(show.id, 2).await;
    assert_eq!(
        renumbered
            .iter()
            .map(|x| (x.number, x.title.as_str(), x.rating))
            .collect::<Vec<_>>(),
        [(1, "Hello, Ms. Cobel", Some(8.6))]
    );
    let specials = fixture.episodes(show.id, 0).await;
    assert_eq!(
        specials
            .iter()
            .map(|x| (x.number, x.title.as_str(), x.rating))
            .collect::<Vec<_>>(),
        [(1, "Inside the Episode", Some(7.2))]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn show_artwork_season_and_specials_posters_and_episode_thumbs_are_kept() {
    let fixture = Fixture::start().await;
    fixture.create_files(&SEVERANCE);
    for name in [
        "poster.jpg",
        "fanart.jpg",
        "clearlogo.png",
        "banner.jpg",
        "season01-poster.jpg",
        "season01-banner.jpg",
        "season02-poster.jpg",
        "season-specials-poster.jpg",
        "season.nfo",
        "unknown.jpg",
        "Season 01/season.nfo",
        "Season 01/unknown.jpg",
    ] {
        fixture.media.create(&format!("{SEVERANCE_FOLDER}/{name}"));
    }
    fixture.media.create(&format!("{S01E01}-thumb.jpg"));
    fixture.media.create(&format!("{S01E02_E03}-thumb.jpg"));
    fixture.media.set_modified(
        &format!("{SEVERANCE_FOLDER}/season-specials-poster.jpg"),
        IMAGE_MODIFIED_AT,
    );

    let show = fixture.reconcile(&SEVERANCE).await;

    assert_eq!(
        artwork(&show.artwork),
        [
            (
                ArtworkKind::Clearlogo,
                fixture
                    .media
                    .path(&format!("{SEVERANCE_FOLDER}/clearlogo.png"))
            ),
            (
                ArtworkKind::Fanart,
                fixture
                    .media
                    .path(&format!("{SEVERANCE_FOLDER}/fanart.jpg"))
            ),
            (
                ArtworkKind::Poster,
                fixture
                    .media
                    .path(&format!("{SEVERANCE_FOLDER}/poster.jpg"))
            ),
        ]
    );
    let seasons = application::list_seasons::call(&fixture.server.state, show.id)
        .await
        .unwrap();
    assert_eq!(
        seasons
            .iter()
            .map(|x| (x.number, artwork(&x.artwork)))
            .collect::<Vec<_>>(),
        [
            (
                1,
                vec![(
                    ArtworkKind::Poster,
                    fixture
                        .media
                        .path(&format!("{SEVERANCE_FOLDER}/season01-poster.jpg"))
                )]
            ),
            (
                2,
                vec![(
                    ArtworkKind::Poster,
                    fixture
                        .media
                        .path(&format!("{SEVERANCE_FOLDER}/season02-poster.jpg"))
                )]
            ),
            (
                0,
                vec![(
                    ArtworkKind::Poster,
                    fixture
                        .media
                        .path(&format!("{SEVERANCE_FOLDER}/season-specials-poster.jpg"))
                )]
            ),
        ]
    );
    assert_eq!(
        seasons[2].artwork[0].modified_at,
        IMAGE_MODIFIED_AT.parse::<Timestamp>().unwrap()
    );
    let episodes = fixture.episodes(show.id, 1).await;
    assert_eq!(
        episodes
            .iter()
            .map(|x| (x.number, artwork(&x.artwork)))
            .collect::<Vec<_>>(),
        [
            (
                1,
                vec![(
                    ArtworkKind::Thumb,
                    fixture.media.path(&format!("{S01E01}-thumb.jpg"))
                )]
            ),
            (
                2,
                vec![(
                    ArtworkKind::Thumb,
                    fixture.media.path(&format!("{S01E02_E03}-thumb.jpg"))
                )]
            ),
            (
                3,
                vec![(
                    ArtworkKind::Thumb,
                    fixture.media.path(&format!("{S01E02_E03}-thumb.jpg"))
                )]
            ),
        ]
    );
    assert_eq!(fixture.episodes(show.id, 2).await[0].artwork, []);
    assert_eq!(fixture.problems().await, []);
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_picks_up_changed_nfos_and_images() {
    let fixture = Fixture::start().await;
    fixture.create_files(&ANDOR);
    fixture.write_nfo(&format!("{ANDOR_FOLDER}/tvshow.nfo"), "tvshow");
    fixture.write_nfo(&format!("{KASSA}.nfo"), "episode");
    fixture.media.create(&format!("{ANDOR_FOLDER}/poster.jpg"));
    fixture.media.create(&format!("{ANDOR_FOLDER}/fanart.jpg"));
    fixture.reconcile(&ANDOR).await;

    fixture.media.write(
        &format!("{ANDOR_FOLDER}/tvshow.nfo"),
        &support::fixture("sonarr/nfo/tvshow.nfo")
            .replace("<status>Ended</status>", "<status>Continuing</status>"),
    );
    fixture.media.set_modified(
        &format!("{ANDOR_FOLDER}/tvshow.nfo"),
        "2026-10-30T08:00:00Z",
    );
    fixture.media.write(
        &format!("{KASSA}.nfo"),
        &support::fixture("sonarr/nfo/episode.nfo")
            .replace("<rating>7.5</rating>", "<rating>7.7</rating>"),
    );
    fixture
        .media
        .set_modified(&format!("{KASSA}.nfo"), "2026-10-30T08:00:00Z");
    fixture.media.remove(&format!("{ANDOR_FOLDER}/fanart.jpg"));
    fixture
        .media
        .set_modified(&format!("{ANDOR_FOLDER}/poster.jpg"), IMAGE_MODIFIED_AT);
    fixture
        .media
        .create(&format!("{ANDOR_FOLDER}/season01-poster.jpg"));
    fixture.media.create(&format!("{KASSA}-thumb.jpg"));
    let show = fixture.reconcile(&ANDOR).await;

    assert_eq!(show.status.as_deref(), Some("Continuing"));
    assert_eq!(show.actors.len(), 3);
    assert_eq!(
        show.artwork
            .iter()
            .map(|x| (x.kind, x.modified_at))
            .collect::<Vec<_>>(),
        [(
            ArtworkKind::Poster,
            IMAGE_MODIFIED_AT.parse::<Timestamp>().unwrap()
        )]
    );
    let seasons = application::list_seasons::call(&fixture.server.state, show.id)
        .await
        .unwrap();
    assert_eq!(seasons[0].artwork.len(), 1);
    let episodes = fixture.episodes(show.id, 1).await;
    assert_eq!(episodes[0].rating, Some(7.7));
    assert_eq!(episodes[0].artwork.len(), 1);
    fixture.stop().await;
}

fn artwork(artwork: &[application::list_movies::Artwork]) -> Vec<(ArtworkKind, String)> {
    artwork.iter().map(|x| (x.kind, x.path.clone())).collect()
}

fn actor(name: &str, role: &str, order: i64) -> Actor {
    Actor {
        name: name.to_owned(),
        role: Some(role.to_owned()),
        order: Some(order),
    }
}
