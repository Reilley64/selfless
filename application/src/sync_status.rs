use crate::DbContext;
use crate::Reconciles;
use crate::SourceHealth;
use domain::Episode;
use domain::HeldRemoval as StoredHeldRemoval;
use domain::InboxRow;
use domain::InboxRowStatus;
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
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to read the sync status")]
pub struct Error;

pub const PROBLEMS_PER_PAGE: usize = 50;

pub struct SyncStatus {
    pub radarr: SourceStatus,
    pub sonarr: SourceStatus,
    pub failed_rows: Vec<FailedRow>,
    pub problems: Vec<Problem>,
    pub problem_page: usize,
    pub problem_pages: usize,
}

pub struct FailedRow {
    pub id: Uuid,
    pub source: Source,
    pub event_type: Option<String>,
    pub error: String,
    pub attempts: i64,
    pub received_at: Timestamp,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub kind: SyncProblemKind,
    pub item: ProblemItem,
    pub path: String,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProblemItem {
    Movie {
        title: String,
        id: Uuid,
    },
    Show {
        title: String,
        id: Uuid,
    },
    Episode {
        show: String,
        season: i64,
        number: i64,
        title: String,
        id: Uuid,
    },
}

#[derive(Debug)]
pub struct SourceStatus {
    pub last_api_call: Option<ApiCall>,
    pub kodi_metadata: Option<bool>,
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
    pub reconcile_progress: Option<ReconcileProgress>,
    pub last_reconcile: Option<LastReconcile>,
    pub held_removal: Option<HeldRemoval>,
    pub visible: VisibleCounts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiCall {
    Succeeded,
    Failed(String),
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

pub async fn call(
    dependencies: impl Into<Dependencies>,
    problem_kind: Option<SyncProblemKind>,
    problem_page: usize,
) -> Result<SyncStatus, Error> {
    let Dependencies {
        mut db_context,
        reconciles,
        source_health,
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
    let failed_rows = InboxRow::filter(InboxRow::fields().status().eq(InboxRowStatus::Failed))
        .order_by(InboxRow::fields().received_at().asc())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| FailedRow {
            id: x.id,
            source: x.source,
            event_type: x.event_type,
            error: x.error.unwrap_or_default(),
            attempts: x.attempts,
            received_at: x.received_at,
        })
        .collect();
    let (problems, problem_page, problem_pages) =
        problems(&mut db_context, problem_kind, problem_page).await?;
    Ok(SyncStatus {
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
        failed_rows,
        problems,
        problem_page,
        problem_pages,
    })
}

async fn source_status(
    db_context: &mut DbContext,
    reconciles: &Reconciles,
    source_health: &SourceHealth,
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

async fn problems(
    db_context: &mut DbContext,
    kind: Option<SyncProblemKind>,
    page: usize,
) -> Result<(Vec<Problem>, usize, usize), Error> {
    let mut stored = SyncProblem::all();
    if let Some(kind) = kind {
        stored = stored.filter(SyncProblem::fields().kind().eq(kind));
    }
    let count = stored
        .clone()
        .count()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let pages = usize::try_from(count)
        .map_err(Error::caused_by)?
        .div_ceil(PROBLEMS_PER_PAGE)
        .max(1);
    let page = page.clamp(1, pages);
    let stored = stored
        .order_by(SyncProblem::fields().path().asc())
        .order_by(SyncProblem::fields().id().asc())
        .limit(PROBLEMS_PER_PAGE)
        .offset((page - 1) * PROBLEMS_PER_PAGE)
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
        let mut items = movies.chain(shows).chain(episodes).collect::<Vec<_>>();
        items.sort();
        for item in items {
            problems.push(Problem {
                kind: problem.kind,
                item,
                path: problem.path.clone(),
            });
        }
    }
    Ok((problems, page, pages))
}
