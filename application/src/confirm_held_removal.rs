use crate::DbContext;
use crate::Reconciles;
use crate::SourceHealth;
use crate::ports::ListRadarrMovies;
use crate::ports::ListSonarrSeries;
use crate::reconcile;
use domain::HeldRemoval;
use domain::Source;
use std::collections::HashSet;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub list_radarr_movies: ListRadarrMovies,
    pub list_sonarr_series: ListSonarrSeries,
    pub reconciles: Reconciles,
    pub source_health: SourceHealth,
}

#[ohno::error]
#[display("failed to confirm a held removal")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        list_radarr_movies,
        list_sonarr_series,
        reconciles,
        source_health,
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
    let listed = match source {
        Source::Radarr => source_health
            .api_call(source, list_radarr_movies.call().await)
            .map_err(Error::caused_by)?
            .into_iter()
            .map(|x| x.tmdb_id)
            .collect::<HashSet<_>>(),
        Source::Sonarr => source_health
            .api_call(source, list_sonarr_series.call().await)
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
        .map_err(Error::caused_by)
}
