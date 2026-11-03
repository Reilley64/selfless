use crate::ports::persistence::DbScope;
use domain::Owner;
use domain::SyncProblem;
use domain::SyncProblemKind;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(crate) enum ProblemOwner {
    Item(Owner),
    MediaFile(Uuid),
}

pub(crate) async fn replace(
    scope: &mut impl DbScope,
    owner: ProblemOwner,
    kind: SyncProblemKind,
    path: Option<&str>,
) -> toasty::Result<()> {
    let owner_filter = match owner {
        ProblemOwner::Item(owner) => SyncProblem::owner_filter(owner),
        ProblemOwner::MediaFile(id) => SyncProblem::fields().media_file_id().eq(id),
    };
    let stored = scope
        .sync_problems()
        .filter(owner_filter.and(SyncProblem::fields().kind().eq(kind)))
        .exec(scope.executor_mut())
        .await?;
    if stored.iter().map(|x| x.path.as_str()).eq(path) {
        return Ok(());
    }
    for problem in stored {
        problem.delete().exec(scope.executor_mut()).await?;
    }
    let Some(path) = path else {
        return Ok(());
    };
    let (item, media_file_id) = match owner {
        ProblemOwner::Item(owner) => (Some(owner), None),
        ProblemOwner::MediaFile(id) => (None, Some(id)),
    };
    scope
        .sync_problems()
        .create()
        .movie_id(item.and_then(Owner::movie_id))
        .show_id(item.and_then(Owner::show_id))
        .media_file_id(media_file_id)
        .kind(kind)
        .path(path)
        .exec(scope.executor_mut())
        .await?;
    Ok(())
}
