use crate::DbContext;
use crate::unsampled;
use domain::InboxRow;
use domain::InboxRowStatus;
use jiff::Timestamp;
use tracing::Instrument;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to read the next inbox attempt")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Option<Timestamp>, Error> {
    let mut db_context = dependencies.into().db_context;
    let next = InboxRow::filter(InboxRow::fields().status().eq(InboxRowStatus::Pending))
        .order_by(InboxRow::fields().next_attempt_at().asc())
        .first()
        .exec(db_context.executor_mut())
        .instrument(unsampled::inbox_poll())
        .await
        .map_err(Error::caused_by)?;
    Ok(next.map(|x| x.next_attempt_at))
}
