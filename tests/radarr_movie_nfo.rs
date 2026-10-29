mod support;

use application::list_movies::Actor;
use application::list_movies::Movie;
use application::sync_status::Problem;
use application::sync_status::ProblemItem;
use domain::ArtworkKind;
use domain::SyncProblemKind;
use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::civil::date;
use reqwest::Method;
use reqwest::StatusCode;
use support::MediaFolder;
use support::RADARR_WEBHOOK_PASSWORD;
use support::StubRadarr;
use support::TestDatabase;
use support::TestServer;
use support::eventually;
use support::fixture;

const RADARR_ID: i64 = 42;
const FILE: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";
const FOLDER: &str = "Movies/Arrival (2016)";
const POSTER_MODIFIED_AT: &str = "2026-10-28T12:30:00Z";
const NFO: &str = "Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.nfo";

struct Fixture {
    database: TestDatabase,
    server: TestServer,
    radarr: StubRadarr,
    media: MediaFolder,
}

impl Fixture {
    async fn start() -> Self {
        let database = TestDatabase::create().await;
        let radarr = StubRadarr::start().await;
        let server = TestServer::start_with_radarr(database.url(), &radarr).await;
        let media = MediaFolder::new();
        media.create(FILE);
        radarr
            .serve_movie(
                RADARR_ID,
                StatusCode::OK,
                &media.fixture("radarr/api/movie.json"),
            )
            .await;
        Self {
            database,
            server,
            radarr,
            media,
        }
    }

    async fn download(&self) -> Movie {
        let status = self
            .server
            .send_radarr_webhook(
                Method::POST,
                Some(("radarr", RADARR_WEBHOOK_PASSWORD)),
                &fixture("radarr/webhooks/download.json"),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        eventually(async || self.movies().await.pop()).await
    }

    async fn movies(&self) -> Vec<Movie> {
        application::list_movies::call(&self.server.state)
            .await
            .unwrap()
    }

    async fn reconcile(&self) -> Movie {
        self.radarr
            .serve_movies(
                StatusCode::OK,
                &format!("[{}]", self.media.fixture("radarr/api/movie.json")),
            )
            .await;
        self.server.clock.advance(SignedDuration::from_mins(1));
        let started_at = self.server.clock.now();
        application::sync_now::call(&self.server.state);
        eventually(async || {
            application::sync_status::call(&self.server.state, None, 1)
                .await
                .unwrap()
                .radarr
                .last_reconcile
                .filter(|x| x.started_at == started_at)
        })
        .await;
        self.movies().await.pop().unwrap()
    }

    fn nfo(&self, name: &str) -> String {
        fixture(&format!("radarr/nfo/{name}.nfo"))
    }

    fn write_nfo(&self, name: &str) {
        self.media.write(NFO, &self.nfo(name));
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
async fn a_movie_takes_its_metadata_from_its_nfo() {
    let fixture = Fixture::start().await;
    fixture.write_nfo("movie");

    let movie = fixture.download().await;

    assert_eq!(movie.title, "Arrival");
    assert_eq!(movie.sort_title.as_deref(), Some("arrival"));
    assert_eq!(movie.original_title.as_deref(), Some("Arrival"));
    assert_eq!(movie.year, 2016);
    assert_eq!(
        movie.overview.as_deref(),
        Some(
            "When twelve mysterious spacecraft appear around the world, linguistics professor \
             Louise Banks is tasked with interpreting the language of the apparent alien visitors."
        )
    );
    assert_eq!(movie.runtime_minutes, Some(116));
    assert_eq!(movie.certification.as_deref(), Some("PG-13"));
    assert_eq!(movie.premiered, Some(date(2016, 11, 10)));
    assert_eq!(movie.studio.as_deref(), Some("FilmNation Entertainment"));
    assert_eq!(movie.genres, ["Drama", "Science Fiction", "Mystery"]);
    assert_eq!(movie.rating, Some(7.6));
    assert_eq!(movie.directors, ["Denis Villeneuve"]);
    assert_eq!(
        movie.actors,
        [
            actor("Amy Adams", "Louise Banks", 0),
            actor("Jeremy Renner", "Ian Donnelly", 1),
            actor("Forest Whitaker", "Colonel Weber", 2),
        ]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn an_nfo_with_trailing_url_lines_is_read() {
    let fixture = Fixture::start().await;
    fixture.write_nfo("movie-with-urls");

    let movie = fixture.download().await;

    assert_eq!(movie.sort_title.as_deref(), Some("arrival"));
    assert_eq!(movie.directors, ["Denis Villeneuve"]);
    assert_eq!(fixture.problems().await, []);
    fixture.stop().await;
}

#[tokio::test]
async fn a_movie_without_an_nfo_takes_its_metadata_from_radarr() {
    let fixture = Fixture::start().await;

    let movie = fixture.download().await;

    assert_radarr_metadata(&movie);
    assert_eq!(fixture.problems().await, []);
    fixture.stop().await;
}

#[tokio::test]
async fn an_unreadable_nfo_falls_back_to_radarr_and_records_a_problem() {
    let fixture = Fixture::start().await;
    fixture.write_nfo("movie-unreadable");

    let movie = fixture.download().await;

    assert_radarr_metadata(&movie);
    assert_eq!(
        fixture.problems().await,
        [Problem {
            kind: SyncProblemKind::UnreadableNfo,
            item: ProblemItem::Movie {
                title: "Arrival".to_owned(),
                id: movie.id,
            },
            path: fixture.media.path(NFO),
        }]
    );
    fixture.stop().await;
}

#[tokio::test]
async fn the_poster_fanart_and_clearlogo_in_the_movie_folder_are_kept() {
    let fixture = Fixture::start().await;
    for name in [
        "poster.jpg",
        "fanart.jpg",
        "clearlogo.png",
        "banner.jpg",
        "landscape.jpg",
        "unknown.jpg",
        "season.nfo",
    ] {
        fixture.media.create(&format!("{FOLDER}/{name}"));
    }
    fixture
        .media
        .set_modified(&format!("{FOLDER}/poster.jpg"), POSTER_MODIFIED_AT);

    let movie = fixture.download().await;

    assert_eq!(
        movie
            .artwork
            .iter()
            .map(|x| (x.kind, x.path.clone()))
            .collect::<Vec<_>>(),
        [
            (
                ArtworkKind::Clearlogo,
                fixture.media.path(&format!("{FOLDER}/clearlogo.png"))
            ),
            (
                ArtworkKind::Fanart,
                fixture.media.path(&format!("{FOLDER}/fanart.jpg"))
            ),
            (
                ArtworkKind::Poster,
                fixture.media.path(&format!("{FOLDER}/poster.jpg"))
            ),
        ]
    );
    assert_eq!(
        movie.artwork[2].modified_at,
        POSTER_MODIFIED_AT.parse::<Timestamp>().unwrap()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_picks_up_a_changed_nfo() {
    let fixture = Fixture::start().await;
    fixture.write_nfo("movie");
    fixture.download().await;

    fixture.media.write(
        NFO,
        &fixture.nfo("movie").replace(
            "<title>Arrival</title>",
            "<title>Arrival (Director's Cut)</title>",
        ),
    );
    fixture.media.set_modified(NFO, "2026-10-30T08:00:00Z");
    let movie = fixture.reconcile().await;

    assert_eq!(movie.title, "Arrival (Director's Cut)");
    assert_eq!(movie.actors.len(), 3);
    fixture.stop().await;
}

#[tokio::test]
async fn a_reconcile_picks_up_added_changed_and_removed_images() {
    let fixture = Fixture::start().await;
    fixture.media.create(&format!("{FOLDER}/poster.jpg"));
    fixture.media.create(&format!("{FOLDER}/fanart.jpg"));
    fixture.download().await;

    fixture.media.remove(&format!("{FOLDER}/fanart.jpg"));
    fixture.media.create(&format!("{FOLDER}/clearlogo.png"));
    fixture
        .media
        .set_modified(&format!("{FOLDER}/poster.jpg"), POSTER_MODIFIED_AT);
    let movie = fixture.reconcile().await;

    assert_eq!(
        movie.artwork.iter().map(|x| x.kind).collect::<Vec<_>>(),
        [ArtworkKind::Clearlogo, ArtworkKind::Poster]
    );
    assert_eq!(
        movie.artwork[1].modified_at,
        POSTER_MODIFIED_AT.parse::<Timestamp>().unwrap()
    );
    fixture.stop().await;
}

fn assert_radarr_metadata(movie: &Movie) {
    assert_eq!(movie.title, "Arrival");
    assert_eq!(movie.year, 2016);
    assert_eq!(movie.genres, ["Drama", "Science Fiction", "Mystery"]);
    assert_eq!(
        movie.overview.as_deref(),
        Some(
            "Taking place after alien crafts land around the world, an expert linguist is \
             recruited by the military to determine whether they come in peace or are a threat."
        )
    );
    assert_eq!(movie.sort_title, None);
    assert_eq!(movie.actors, []);
}

fn actor(name: &str, role: &str, order: i64) -> Actor {
    Actor {
        name: name.to_owned(),
        role: Some(role.to_owned()),
        order: Some(order),
    }
}
