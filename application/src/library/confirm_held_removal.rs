mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::reconcile;
use crate::ports::persistence::DbScope;
use domain::HeldRemoval;
use domain::Source;
use std::collections::HashSet;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        list_radarr_movies,
        list_sonarr_shows,
        reconciles,
        source_health,
    } = dependencies.into();
    let _running = reconciles.source(source).running.lock().await;
    let Some(held_removal) = db_context
        .held_removals()
        .filter(HeldRemoval::fields().source().eq(source))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output);
    };
    let listed = match source {
        Source::Radarr => source_health
            .api_call(source, list_radarr_movies.call().await)
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.tmdb_id)
            .collect::<HashSet<_>>(),
        Source::Sonarr => source_health
            .api_call(source, list_sonarr_shows.call().await)
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.tvdb_id)
            .collect(),
    };
    reconcile::remove(
        &mut db_context,
        source,
        held_removal.item_ids.clone(),
        &listed,
    )
    .await
    .map_err(Error::caused_by)?;
    held_removal
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
