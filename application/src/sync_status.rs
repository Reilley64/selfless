use crate::DbContext;
use domain::Source;
use domain::SourceState;
use jiff::Timestamp;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to read the sync status")]
pub struct Error;

pub struct SyncStatus {
    pub radarr: SourceStatus,
}

#[derive(Default)]
pub struct SourceStatus {
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<SyncStatus, Error> {
    let mut db_context = dependencies.into().db_context;
    let radarr = SourceState::filter_by_source(Source::Radarr)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .map(|x| SourceStatus {
            last_webhook_at: x.last_webhook_at,
            rejected_webhooks: x.rejected_webhooks,
        })
        .unwrap_or_default();
    Ok(SyncStatus { radarr })
}
