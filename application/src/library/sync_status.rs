mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::ApiCall;
pub use output::HeldRemoval;
pub use output::LastReconcile;
pub use output::Output;
pub use output::Problem;
pub use output::ProblemItem;
pub use output::ReconcileProgress;
pub use output::SourceStatus;
pub use output::VisibleCounts;

use crate::library::Reconciles;
use crate::library::SourceHealth;
use crate::library::source_health::ApiCallResult;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::Episode;
use domain::HeldRemoval as StoredHeldRemoval;
use domain::Movie;
use domain::Show;
use domain::Source;
use domain::SourceState;
use domain::SyncProblem;
use std::time::Duration;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        reconciles,
        source_health,
    } = dependencies.into();
    let movies = db_context
        .movies()
        .filter(Movie::in_library_filter())
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let shows = db_context
        .shows()
        .filter(Show::in_library_filter())
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let episodes = db_context
        .episodes()
        .filter(Episode::in_library_filter())
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let problems = problems(&mut db_context).await?;
    Ok(Output {
        radarr: source_status(
            &mut db_context,
            &reconciles,
            &source_health,
            Source::Radarr,
            VisibleCounts::Movies(movies),
        )
        .await?,
        sonarr: source_status(
            &mut db_context,
            &reconciles,
            &source_health,
            Source::Sonarr,
            VisibleCounts::Shows { shows, episodes },
        )
        .await?,
        problems,
    })
}

async fn source_status(
    db_context: &mut DbContext,
    reconciles: &Reconciles,
    source_health: &SourceHealth,
    source: Source,
    visible: VisibleCounts,
) -> Result<SourceStatus, Error> {
    let state = db_context
        .source_states()
        .filter(SourceState::fields().source().eq(source))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let held_removal = db_context
        .held_removals()
        .filter(StoredHeldRemoval::fields().source().eq(source))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let health = source_health.health(source);
    Ok(SourceStatus {
        last_api_call: health.last_api_call.map(|x| match x {
            ApiCallResult::Succeeded => ApiCall::Succeeded,
            ApiCallResult::Failed(error) => ApiCall::Failed(error),
        }),
        kodi_metadata: health.kodi_metadata,
        last_webhook_at: state.as_ref().and_then(|x| x.last_webhook_at),
        rejected_webhooks: state.as_ref().map_or(0, |x| x.rejected_webhooks),
        reconcile_progress: reconciles.progress(source).map(|x| ReconcileProgress {
            imported: x.imported,
            total: x.total,
        }),
        last_reconcile: state.and_then(|x| {
            Some(LastReconcile {
                started_at: x.last_reconcile_started_at?,
                duration: Duration::from_millis(u64::try_from(x.last_reconcile_duration_ms?).ok()?),
                result: x.last_reconcile_result?,
            })
        }),
        held_removal: held_removal.map(|x| HeldRemoval {
            count: x.item_ids.len(),
        }),
        visible,
    })
}

async fn problems(db_context: &mut DbContext) -> Result<Vec<Problem>, Error> {
    let stored = db_context
        .sync_problems()
        .all()
        .include(SyncProblem::fields().movie())
        .include(SyncProblem::fields().show())
        .include(SyncProblem::fields().media_file().movies())
        .include(
            SyncProblem::fields()
                .media_file()
                .episodes()
                .season()
                .show(),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let mut problems = Vec::new();
    for problem in stored {
        let media_file = problem.media_file.get().as_ref();
        let movies = problem
            .movie
            .get()
            .iter()
            .chain(media_file.into_iter().flat_map(|x| x.movies.get()))
            .map(|x| ProblemItem::Movie {
                title: x.title.clone(),
                id: x.id,
            });
        let shows = problem.show.get().iter().map(|x| ProblemItem::Show {
            title: x.title.clone(),
            id: x.id,
        });
        let episodes = media_file
            .into_iter()
            .flat_map(|x| x.episodes.get())
            .map(|x| {
                let season = x.season.get();
                ProblemItem::Episode {
                    show: season.show.get().title.clone(),
                    season: season.number,
                    number: x.number,
                    title: x.title.clone(),
                    id: x.id,
                }
            });
        for item in movies.chain(shows).chain(episodes).collect::<Vec<_>>() {
            problems.push(Problem {
                kind: problem.kind,
                item,
                path: problem.path.clone(),
            });
        }
    }
    problems.sort_by(|a, b| (&a.path, &a.item).cmp(&(&b.path, &b.item)));
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RadarrError;
    use crate::test::mock_db_context;
    use domain::ReconcileResult;
    use domain::SyncProblemKind;
    use jiff::Timestamp;
    use uuid::Uuid;

    struct Seeded {
        db_context: DbContext,
        episodes: i64,
    }

    impl Seeded {
        async fn new() -> Self {
            Self {
                db_context: mock_db_context().await,
                episodes: 0,
            }
        }

        async fn media_file(&mut self, path: &str) -> Uuid {
            let db_context = &mut self.db_context;
            db_context
                .media_files()
                .create()
                .path(path)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn movie(
            &mut self,
            tmdb_id: i64,
            title: &str,
            media_file_id: Option<Uuid>,
            visible: bool,
        ) -> Uuid {
            let db_context = &mut self.db_context;
            db_context
                .movies()
                .create()
                .tmdb_id(tmdb_id)
                .radarr_id(tmdb_id)
                .title(title)
                .year(2016)
                .genres(Vec::<String>::new())
                .directors(Vec::<String>::new())
                .media_file_id(media_file_id)
                .visible(visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn show(&mut self, tvdb_id: i64, title: &str) -> Uuid {
            let db_context = &mut self.db_context;
            db_context
                .shows()
                .create()
                .tvdb_id(tvdb_id)
                .sonarr_id(tvdb_id)
                .title(title)
                .genres(Vec::<String>::new())
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn season(&mut self, show_id: Uuid, number: i64) -> Uuid {
            let db_context = &mut self.db_context;
            db_context
                .seasons()
                .create()
                .show_id(show_id)
                .number(number)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id
        }

        async fn episode(
            &mut self,
            season_id: Uuid,
            number: i64,
            title: &str,
            media_file_id: Option<Uuid>,
            visible: bool,
        ) {
            self.episodes += 1;
            let db_context = &mut self.db_context;
            db_context
                .episodes()
                .create()
                .season_id(season_id)
                .tvdb_id(self.episodes)
                .number(number)
                .title(title)
                .media_file_id(media_file_id)
                .visible(visible)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        async fn problem(
            &mut self,
            kind: SyncProblemKind,
            path: &str,
            movie_id: Option<Uuid>,
            show_id: Option<Uuid>,
            media_file_id: Option<Uuid>,
        ) {
            let db_context = &mut self.db_context;
            db_context
                .sync_problems()
                .create()
                .kind(kind)
                .path(path)
                .movie_id(movie_id)
                .show_id(show_id)
                .media_file_id(media_file_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        async fn status(&self, source_health: &SourceHealth) -> Output {
            call(Dependencies {
                db_context: self.db_context.clone(),
                reconciles: Reconciles::default(),
                source_health: source_health.clone(),
            })
            .await
            .unwrap()
        }
    }

    fn kinds_paths_and_titles(problems: &[Problem]) -> Vec<(SyncProblemKind, &str, &str)> {
        problems
            .iter()
            .map(|x| {
                let title = match &x.item {
                    ProblemItem::Movie { title, .. }
                    | ProblemItem::Show { title, .. }
                    | ProblemItem::Episode { title, .. } => title.as_str(),
                };
                (x.kind, x.path.as_str(), title)
            })
            .collect()
    }

    #[tokio::test]
    async fn a_media_file_problem_is_listed_with_its_kind_path_and_movie() {
        let mut seeded = Seeded::new().await;
        let arrival = seeded.media_file("/movies/Arrival.mkv").await;
        let dune = seeded.media_file("/movies/Dune.mkv").await;
        seeded.movie(1, "Arrival", Some(arrival), true).await;
        seeded.movie(2, "Dune", Some(dune), false).await;
        seeded
            .problem(
                SyncProblemKind::FfprobeFailed,
                "/movies/Arrival.mkv",
                None,
                None,
                Some(arrival),
            )
            .await;
        seeded
            .problem(
                SyncProblemKind::FileNotFound,
                "/movies/Dune.mkv",
                None,
                None,
                Some(dune),
            )
            .await;

        let status = seeded.status(&SourceHealth::default()).await;

        assert_eq!(
            kinds_paths_and_titles(&status.problems),
            [
                (
                    SyncProblemKind::FfprobeFailed,
                    "/movies/Arrival.mkv",
                    "Arrival"
                ),
                (SyncProblemKind::FileNotFound, "/movies/Dune.mkv", "Dune"),
            ]
        );
    }

    #[tokio::test]
    async fn a_problem_owned_by_a_movie_or_a_show_names_that_item() {
        let mut seeded = Seeded::new().await;
        let arrival = seeded.movie(1, "Arrival", None, true).await;
        let severance = seeded.show(7, "Severance").await;
        seeded
            .problem(
                SyncProblemKind::UnreadableNfo,
                "/movies/Arrival.nfo",
                Some(arrival),
                None,
                None,
            )
            .await;
        seeded
            .problem(
                SyncProblemKind::UnreadableNfo,
                "/tv/Severance/tvshow.nfo",
                None,
                Some(severance),
                None,
            )
            .await;

        let status = seeded.status(&SourceHealth::default()).await;

        assert_eq!(
            status
                .problems
                .iter()
                .map(|x| (x.path.as_str(), &x.item))
                .collect::<Vec<_>>(),
            [
                (
                    "/movies/Arrival.nfo",
                    &ProblemItem::Movie {
                        title: "Arrival".to_owned(),
                        id: arrival
                    }
                ),
                (
                    "/tv/Severance/tvshow.nfo",
                    &ProblemItem::Show {
                        title: "Severance".to_owned(),
                        id: severance
                    }
                ),
            ]
        );
    }

    #[tokio::test]
    async fn a_problem_on_a_file_holding_two_episodes_is_listed_for_each_episode() {
        let mut seeded = Seeded::new().await;
        let path = "/tv/Severance/Season 01/Severance - S01E02-E03.mkv";
        let file = seeded.media_file(path).await;
        let show = seeded.show(7, "Severance").await;
        let season = seeded.season(show, 1).await;
        seeded
            .episode(season, 3, "In Perpetuity", Some(file), true)
            .await;
        seeded
            .episode(season, 2, "Half Loop", Some(file), true)
            .await;
        seeded
            .problem(SyncProblemKind::FileNotFound, path, None, None, Some(file))
            .await;

        let status = seeded.status(&SourceHealth::default()).await;

        assert_eq!(
            status
                .problems
                .iter()
                .map(|x| match &x.item {
                    ProblemItem::Episode {
                        show,
                        season,
                        number,
                        title,
                        ..
                    } => (
                        x.kind,
                        x.path.as_str(),
                        show.as_str(),
                        *season,
                        *number,
                        title.as_str()
                    ),
                    other => panic!("the problem names an episode, not {other:?}"),
                })
                .collect::<Vec<_>>(),
            [
                (
                    SyncProblemKind::FileNotFound,
                    path,
                    "Severance",
                    1,
                    2,
                    "Half Loop"
                ),
                (
                    SyncProblemKind::FileNotFound,
                    path,
                    "Severance",
                    1,
                    3,
                    "In Perpetuity"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn only_movies_shows_and_episodes_in_the_library_are_counted() {
        let mut seeded = Seeded::new().await;
        seeded.movie(1, "Arrival", None, true).await;
        seeded.movie(2, "Dune", None, false).await;
        let severance = seeded.show(7, "Severance").await;
        let severance_season = seeded.season(severance, 1).await;
        seeded
            .episode(severance_season, 1, "Good News About Hell", None, true)
            .await;
        seeded
            .episode(severance_season, 2, "Half Loop", None, false)
            .await;
        let andor = seeded.show(8, "Andor").await;
        let andor_season = seeded.season(andor, 1).await;
        seeded.episode(andor_season, 1, "Kassa", None, false).await;

        let status = seeded.status(&SourceHealth::default()).await;

        assert_eq!(status.radarr.visible, VisibleCounts::Movies(1));
        assert_eq!(
            status.sonarr.visible,
            VisibleCounts::Shows {
                shows: 1,
                episodes: 1
            }
        );
    }

    #[tokio::test]
    async fn the_stored_webhook_and_reconcile_state_is_shown_per_source() {
        let mut seeded = Seeded::new().await;
        let webhook_at: Timestamp = "2026-06-01T12:00:00Z".parse().unwrap();
        let reconcile_at: Timestamp = "2026-06-01T13:00:00Z".parse().unwrap();
        let db_context = &mut seeded.db_context;
        db_context
            .source_states()
            .create()
            .source(Source::Sonarr)
            .last_webhook_at(Some(webhook_at))
            .rejected_webhooks(3)
            .last_reconcile_started_at(Some(reconcile_at))
            .last_reconcile_duration_ms(Some(1500))
            .last_reconcile_result(Some(ReconcileResult::Failed))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let status = seeded.status(&SourceHealth::default()).await;

        assert_eq!(status.sonarr.last_webhook_at, Some(webhook_at));
        assert_eq!(status.sonarr.rejected_webhooks, 3);
        let last_reconcile = status.sonarr.last_reconcile.unwrap();
        assert_eq!(last_reconcile.started_at, reconcile_at);
        assert_eq!(last_reconcile.duration, Duration::from_millis(1500));
        assert_eq!(last_reconcile.result, ReconcileResult::Failed);
        assert_eq!(status.radarr.last_webhook_at, None);
        assert_eq!(status.radarr.rejected_webhooks, 0);
        assert!(status.radarr.last_reconcile.is_none());
    }

    #[tokio::test]
    async fn the_last_api_call_is_shown_per_source() {
        let seeded = Seeded::new().await;
        let source_health = SourceHealth::default();
        let _ = source_health.api_call(
            Source::Radarr,
            Err::<(), _>(RadarrError::caused_by("500 Internal Server Error")),
        );
        let _ = source_health.api_call(Source::Sonarr, Ok::<_, RadarrError>(()));

        let status = seeded.status(&source_health).await;

        assert!(
            matches!(&status.radarr.last_api_call, Some(ApiCall::Failed(error)) if error.contains("500")),
            "{:?}",
            status.radarr.last_api_call
        );
        assert_eq!(status.sonarr.last_api_call, Some(ApiCall::Succeeded));
    }
}
