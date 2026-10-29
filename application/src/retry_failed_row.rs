use crate::DbContext;
use crate::InboxWakeup;
use crate::ports::Clock;
use domain::InboxRow;
use domain::InboxRowStatus;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub clock: Clock,
    pub db_context: DbContext,
    pub inbox_wakeup: InboxWakeup,
}

#[ohno::error]
#[display("failed to retry a failed inbox row")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<(), Error> {
    let Dependencies {
        clock,
        mut db_context,
        inbox_wakeup,
    } = dependencies.into();
    let Some(mut row) = InboxRow::filter_by_id(id)
        .filter(InboxRow::fields().status().eq(InboxRowStatus::Failed))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(());
    };
    row.update()
        .status(InboxRowStatus::Pending)
        .next_attempt_at(clock.call())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    inbox_wakeup.wake();
    Ok(())
}
