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
use std::collections::HashMap;
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

pub struct SyncStatus {
    pub radarr: SourceStatus,
    pub sonarr: SourceStatus,
    pub failed_rows: Vec<FailedRow>,
    pub problems: Vec<Problem>,
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

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<SyncStatus, Error> {
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
    let problems = problems(&mut db_context).await?;
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
    let health = source_health.source(source).lock().unwrap().clone();
    Ok(SourceStatus {
        last_api_call: health.last_api_call,
        kodi_metadata: health.kodi_metadata,
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

async fn problems(db_context: &mut DbContext) -> Result<Vec<Problem>, Error> {
    let stored = SyncProblem::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let movie_ids = stored.iter().filter_map(|x| x.movie_id).collect::<Vec<_>>();
    let media_file_ids = stored
        .iter()
        .filter_map(|x| x.media_file_id)
        .collect::<Vec<_>>();
    let mut movies = Vec::new();
    if !movie_ids.is_empty() {
        movies.extend(
            Movie::filter(Movie::fields().id().in_list(movie_ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?,
        );
    }
    let mut episodes = Vec::new();
    if !media_file_ids.is_empty() {
        movies.extend(
            Movie::filter(
                Movie::fields()
                    .media_file_id()
                    .in_list(media_file_ids.clone()),
            )
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?,
        );
        episodes = Episode::filter(Episode::fields().media_file_id().in_list(media_file_ids))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    let seasons = if episodes.is_empty() {
        HashMap::new()
    } else {
        Season::filter(
            Season::fields()
                .id()
                .in_list(episodes.iter().map(|x| x.season_id).collect::<Vec<_>>()),
        )
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| (x.id, x))
        .collect::<HashMap<_, _>>()
    };
    let show_ids = seasons
        .values()
        .map(|x| x.show_id)
        .chain(stored.iter().filter_map(|x| x.show_id))
        .collect::<Vec<_>>();
    let shows = if show_ids.is_empty() {
        HashMap::new()
    } else {
        Show::filter(Show::fields().id().in_list(show_ids))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| (x.id, x.title))
            .collect::<HashMap<_, _>>()
    };
    let mut problems = Vec::new();
    for problem in stored {
        let movie_items = movies
            .iter()
            .filter(|x| {
                problem.movie_id == Some(x.id)
                    || problem.media_file_id.is_some() && problem.media_file_id == x.media_file_id
            })
            .map(|x| ProblemItem::Movie {
                title: x.title.clone(),
                id: x.id,
            });
        let episode_items = episodes
            .iter()
            .filter(|x| problem.media_file_id.is_some() && problem.media_file_id == x.media_file_id)
            .filter_map(|x| {
                let season = seasons.get(&x.season_id)?;
                Some(ProblemItem::Episode {
                    show: shows.get(&season.show_id)?.clone(),
                    season: season.number,
                    number: x.number,
                    title: x.title.clone(),
                    id: x.id,
                })
            });
        let show_items = problem.show_id.and_then(|id| {
            Some(ProblemItem::Show {
                title: shows.get(&id)?.clone(),
                id,
            })
        });
        for item in movie_items
            .chain(show_items)
            .chain(episode_items)
            .collect::<Vec<_>>()
        {
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
