use crate::ports::persistence::DbScope;
use domain::Source;
use domain::SourceState;

pub(crate) async fn upsert(
    scope: &mut impl DbScope,
    source: Source,
) -> toasty::Result<SourceState> {
    SourceState::upsert_by_source(source)
        .rejected_webhooks(0)
        .or_ignore()
        .exec(scope.executor_mut())
        .await?;
    scope
        .source_states()
        .filter(SourceState::fields().source().eq(source))
        .one()
        .exec(scope.executor_mut())
        .await
}
