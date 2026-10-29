use crate::DbContext;
use crate::ports::Clock;
use crate::unsampled;
use jiff::SignedDuration;
use tracing::Instrument;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to delete old inbox rows")]
pub struct Error;

const KEPT_FOR: SignedDuration = SignedDuration::from_hours(30 * 24);

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<u64, Error> {
    let Dependencies {
        clock,
        mut db_context,
    } = dependencies.into();
    let received_before = clock.call() - KEPT_FOR;
    toasty::sql::statement(
        "DELETE FROM inbox_attempts WHERE inbox_row_id IN (SELECT id FROM inbox_rows \
         WHERE status IN ('processed', 'ignored', 'resolved') AND received_at <= $1)",
    )
    .bind(received_before)
    .exec(db_context.executor_mut())
    .instrument(unsampled::inbox_poll())
    .await
    .map_err(Error::caused_by)?;
    toasty::sql::statement(
        "DELETE FROM inbox_rows WHERE status IN ('processed', 'ignored', 'resolved') \
         AND received_at <= $1",
    )
    .bind(received_before)
    .exec(db_context.executor_mut())
    .instrument(unsampled::inbox_poll())
    .await
    .map_err(Error::caused_by)
}
