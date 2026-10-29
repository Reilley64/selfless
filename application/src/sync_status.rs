use crate::DbContext;
use crate::Reconciles;
use domain::Episode;
use domain::HeldRemoval as StoredHeldRemoval;
use domain::Movie;
use domain::ReconcileResult;
use domain::Season;
use domain::Show;
use domain::Source;
use domain::SourceState;
use domain::SyncProblem;
use domain::SyncProblemKind;
use jiff::Timestamp;
use std::time::Duration;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub reconciles: Reconciles,
}

#[ohno::error]
#[display("failed to read the sync status")]
pub struct Error;

pub struct SyncStatus {
    pub radarr: SourceStatus,
    pub sonarr: SourceStatus,
    pub problems: Vec<Problem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: SyncProblemKind,
    pub path: String,
}

#[derive(Debug)]
pub struct SourceStatus {
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
    pub reconcile_progress: Option<ReconcileProgress>,
    pub last_reconcile: Option<LastReconcile>,
    pub held_removal: Option<HeldRemoval>,
    pub visible: VisibleCounts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconcileProgress {
    pub imported: usize,
    pub total: Option<usize>,
}

#[derive(Debug)]
pub struct LastReconcile {
    pub started_at: Timestamp,
    pub duration: Duration,
    pub result: ReconcileResult,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HeldRemoval {
    pub count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VisibleCounts {
    Movies(u64),
    Shows { shows: u64, episodes: u64 },
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<SyncStatus, Error> {
    let Dependencies {
        mut db_context,
        reconciles,
    } = dependencies.into();
    let movies = Movie::filter(Movie::fields().visible().eq(true))
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let shows = Show::filter(
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
    let episodes = Episode::filter(Episode::fields().visible().eq(true))
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let problems = SyncProblem::all()
        .order_by(SyncProblem::fields().path().asc())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| Problem {
            kind: x.kind,
            path: x.path,
        })
        .collect();
    Ok(SyncStatus {
        radarr: source_status(
            &mut db_context,
            &reconciles,
            Source::Radarr,
            VisibleCounts::Movies(movies),
        )
        .await?,
        sonarr: source_status(
            &mut db_context,
            &reconciles,
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
    source: Source,
    visible: VisibleCounts,
) -> Result<SourceStatus, Error> {
    let state = SourceState::filter_by_source(source)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let held_removal = StoredHeldRemoval::filter_by_source(source)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(SourceStatus {
        last_webhook_at: state.as_ref().and_then(|x| x.last_webhook_at),
        rejected_webhooks: state.as_ref().map_or(0, |x| x.rejected_webhooks),
        reconcile_progress: *reconciles.source(source).progress.lock().unwrap(),
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
