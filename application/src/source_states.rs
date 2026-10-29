use domain::Source;
use domain::SourceState;

pub(crate) async fn upsert(
    db: &mut dyn toasty::Executor,
    source: Source,
) -> toasty::Result<SourceState> {
    toasty::sql::statement(
        "INSERT INTO source_states (source, rejected_webhooks) VALUES ($1, 0) \
         ON CONFLICT (source) DO NOTHING",
    )
    .bind(source.name())
    .exec(db)
    .await?;
    SourceState::get_by_source(db, &source).await
}
