use crate::DbContext;
use crate::Reconciles;
use crate::reconcile;
use domain::HeldRemoval;
use domain::Source;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub reconciles: Reconciles,
}

#[ohno::error]
#[display("failed to confirm a held removal")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        reconciles,
    } = dependencies.into();
    let _running = reconciles.source(source).running.lock().await;
    let Some(held_removal) = HeldRemoval::filter_by_source(source)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(());
    };
    reconcile::remove(&mut db_context, source, held_removal.item_ids.clone())
        .await
        .map_err(Error::caused_by)?;
    held_removal
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
