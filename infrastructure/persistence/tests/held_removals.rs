#[path = "../../../test/persistence.rs"]
mod persistence_test;

use application::library::Prober;
use application::library::Reconciles;
use application::library::SourceHealth;
use application::library::confirm_held_removal;
use application::library::reconcile;
use application::library::sync_status;
use application::library::sync_status::HeldRemoval;
use application::ports::CheckRadarrKodiMetadata;
use application::ports::CheckSonarrKodiMetadata;
use application::ports::Clock;
use application::ports::FindMediaFile;
use application::ports::FoundMediaFile;
use application::ports::GetSonarrEpisodes;
use application::ports::ListRadarrMovies;
use application::ports::ListSonarrShows;
use application::ports::ListSubtitleFiles;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::RadarrMovie;
use application::ports::RadarrMovieFile;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::UnreadableNfo;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbScope;
use domain::Source;
use jiff::Timestamp;
use persistence_test::db_context;
use std::sync::Arc;
use std::sync::Mutex;
use test_fixtures::FakeClock;
use test_fixtures::TestDatabase;

const ALL_MOVIES: [i64; 4] = [42, 43, 44, 45];

struct Library {
    database: TestDatabase,
    db_context: DbContext,
    clock: FakeClock,
    reconciles: Reconciles,
    source_health: SourceHealth,
    radarr: Arc<Mutex<Vec<RadarrMovie>>>,
}

impl Library {
    async fn new() -> Self {
        let database = TestDatabase::create().await;
        Self {
            db_context: db_context(&database).await,
            database,
            clock: FakeClock::new(),
            reconciles: Reconciles::default(),
            source_health: SourceHealth::default(),
            radarr: Arc::default(),
        }
    }

    fn radarr_lists(&self, ids: &[i64]) {
        self.radarr_lists_movies(
            ids.iter()
                .map(|x| radarr_movie(*x, x.to_string()))
                .collect(),
        );
    }

    fn radarr_lists_movies(&self, movies: Vec<RadarrMovie>) {
        *self.radarr.lock().unwrap() = movies;
    }

    fn list_radarr_movies(&self) -> ListRadarrMovies {
        let radarr = self.radarr.clone();
        ListRadarrMovies::new(move || {
            let movies = radarr.lock().unwrap().clone();
            Box::pin(async move { Ok(movies) })
        })
    }

    fn list_sonarr_shows() -> ListSonarrShows {
        ListSonarrShows::new(|| Box::pin(async { Ok(Vec::new()) }))
    }

    async fn reconcile(&self) {
        reconcile::call(
            reconcile::Dependencies {
                check_radarr_kodi_metadata: CheckRadarrKodiMetadata::new(|| {
                    Box::pin(async { Ok(true) })
                }),
                check_sonarr_kodi_metadata: CheckSonarrKodiMetadata::new(|| {
                    Box::pin(async { Ok(true) })
                }),
                clock: Clock::new(self.clock.reader()),
                db_context: self.db_context.clone(),
                find_media_file: FindMediaFile::new(|_| {
                    Box::pin(async {
                        Some(FoundMediaFile {
                            size: 1,
                            modified_at: Timestamp::UNIX_EPOCH,
                        })
                    })
                }),
                get_sonarr_episodes: GetSonarrEpisodes::new(|_| Box::pin(async { Ok(Vec::new()) })),
                list_radarr_movies: self.list_radarr_movies(),
                list_sonarr_shows: Self::list_sonarr_shows(),
                list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
                prober: Prober::new(ProbeMediaFile::new(|_| {
                    Box::pin(async { Err(ProbeError::default()) })
                })),
                read_episode_folder: ReadEpisodeFolder::new(|_| {
                    Box::pin(async { Default::default() })
                }),
                read_episode_nfo: ReadEpisodeNfo::new(|_| {
                    Box::pin(async { Err(UnreadableNfo::default()) })
                }),
                read_movie_folder: ReadMovieFolder::new(|_| Box::pin(async { Default::default() })),
                read_movie_nfo: ReadMovieNfo::new(|_| {
                    Box::pin(async { Err(UnreadableNfo::default()) })
                }),
                read_show_folder: ReadShowFolder::new(|_, _| {
                    Box::pin(async { Default::default() })
                }),
                read_show_nfo: ReadShowNfo::new(|_| {
                    Box::pin(async { Err(UnreadableNfo::default()) })
                }),
                reconciles: self.reconciles.clone(),
                source_health: self.source_health.clone(),
            },
            Source::Radarr,
        )
        .await
        .unwrap();
    }

    async fn confirm_held_removal(&self) {
        confirm_held_removal::call(
            confirm_held_removal::Dependencies {
                db_context: self.db_context.clone(),
                list_radarr_movies: self.list_radarr_movies(),
                list_sonarr_shows: Self::list_sonarr_shows(),
                reconciles: self.reconciles.clone(),
                source_health: self.source_health.clone(),
            },
            Source::Radarr,
        )
        .await
        .unwrap();
    }

    async fn held_removal(&self) -> Option<HeldRemoval> {
        sync_status::call(sync_status::Dependencies {
            db_context: self.db_context.clone(),
            reconciles: self.reconciles.clone(),
            source_health: self.source_health.clone(),
        })
        .await
        .unwrap()
        .radarr
        .held_removal
    }

    async fn movie_titles(&self) -> Vec<String> {
        let mut db_context = self.db_context.clone();
        let mut titles = db_context
            .movies()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.title)
            .collect::<Vec<_>>();
        titles.sort();
        titles
    }

    async fn stop(self) {
        drop(self.db_context);
        self.database.drop().await;
    }
}

fn radarr_movie(tmdb_id: i64, title: String) -> RadarrMovie {
    RadarrMovie {
        radarr_id: tmdb_id,
        tmdb_id,
        movie_file: Some(RadarrMovieFile {
            path: format!("/movies/{title}.mkv"),
            date_added: Timestamp::UNIX_EPOCH,
        }),
        title,
        year: 2016,
        overview: None,
        genres: Vec::new(),
    }
}

#[tokio::test]
async fn a_removal_of_more_than_half_the_movies_is_held_until_an_admin_confirms_it() {
    let library = Library::new().await;
    library.radarr_lists(&ALL_MOVIES);
    library.reconcile().await;

    library.radarr_lists(&[42]);
    library.reconcile().await;
    let held = library.held_removal().await;
    let titles_while_held = library.movie_titles().await;
    library.confirm_held_removal().await;

    assert_eq!(held, Some(HeldRemoval { count: 3 }));
    assert_eq!(titles_while_held, ["42", "43", "44", "45"]);
    assert_eq!(library.movie_titles().await, ["42"]);
    assert_eq!(library.held_removal().await, None);
    library.stop().await;
}

#[tokio::test]
async fn confirming_a_held_removal_keeps_the_movies_radarr_lists_again() {
    let library = Library::new().await;
    library.radarr_lists(&ALL_MOVIES);
    library.reconcile().await;
    library.radarr_lists(&[42]);
    library.reconcile().await;

    library.radarr_lists(&[42, 43]);
    library.confirm_held_removal().await;

    assert_eq!(library.movie_titles().await, ["42", "43"]);
    assert_eq!(library.held_removal().await, None);
    library.stop().await;
}

#[tokio::test]
async fn a_radarr_that_lists_as_many_other_movies_holds_the_removal_of_every_stored_movie() {
    let library = Library::new().await;
    library.radarr_lists(&ALL_MOVIES);
    library.reconcile().await;

    library.radarr_lists_movies(
        ALL_MOVIES
            .iter()
            .map(|x| radarr_movie(x + 100, format!("Other {x}")))
            .collect(),
    );
    library.reconcile().await;

    assert_eq!(library.held_removal().await, Some(HeldRemoval { count: 4 }));
    assert_eq!(
        library.movie_titles().await,
        [
            "42", "43", "44", "45", "Other 42", "Other 43", "Other 44", "Other 45"
        ]
    );
    library.stop().await;
}

#[tokio::test]
async fn a_later_reconcile_with_a_smaller_removal_replaces_the_held_one() {
    let library = Library::new().await;
    library.radarr_lists(&ALL_MOVIES);
    library.reconcile().await;
    library.radarr_lists(&[]);
    library.reconcile().await;
    let first = library.held_removal().await;

    library.radarr_lists(&[42]);
    library.reconcile().await;
    let replaced = library.held_removal().await;
    library.confirm_held_removal().await;

    assert_eq!(first, Some(HeldRemoval { count: 4 }));
    assert_eq!(replaced, Some(HeldRemoval { count: 3 }));
    assert_eq!(library.movie_titles().await, ["42"]);
    library.stop().await;
}
