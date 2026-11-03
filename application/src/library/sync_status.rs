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
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::Episode;
use domain::HeldRemoval as StoredHeldRemoval;
use domain::Movie;
use domain::Season;
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
        .filter(Movie::fields().visible().eq(true))
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let shows = db_context
        .shows()
        .filter(
            Show::fields().seasons().any(
                Season::fields()
                    .episodes()
                    .any(Episode::fields().visible().eq(true)),
            ),
        )
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let episodes = db_context
        .episodes()
        .filter(Episode::fields().visible().eq(true))
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
        last_api_call: health.last_api_call,
        kodi_metadata: health.kodi_metadata,
        last_webhook_at: state.as_ref().and_then(|x| x.last_webhook_at),
        rejected_webhooks: state.as_ref().map_or(0, |x| x.rejected_webhooks),
        reconcile_progress: reconciles.progress(source),
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
