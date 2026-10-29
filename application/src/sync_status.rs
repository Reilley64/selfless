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
    pub sonarr: SourceStatus,
}

#[derive(Default)]
pub struct SourceStatus {
    pub last_webhook_at: Option<Timestamp>,
    pub rejected_webhooks: i64,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<SyncStatus, Error> {
    let mut db_context = dependencies.into().db_context;
    Ok(SyncStatus {
        radarr: source_status(&mut db_context, Source::Radarr).await?,
        sonarr: source_status(&mut db_context, Source::Sonarr).await?,
    })
}

async fn source_status(db_context: &mut DbContext, source: Source) -> Result<SourceStatus, Error> {
    Ok(SourceState::filter_by_source(source)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .map(|x| SourceStatus {
            last_webhook_at: x.last_webhook_at,
            rejected_webhooks: x.rejected_webhooks,
        })
        .unwrap_or_default())
}
