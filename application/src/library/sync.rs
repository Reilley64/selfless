pub(crate) mod movies;
pub(crate) mod shows;

use crate::ports::AcquireLock;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::Movie;
use domain::Show;
use domain::Source;
use std::collections::HashSet;
use uuid::Uuid;

#[ohno::error]
#[display("failed to sync the library")]
pub(crate) struct Error;

pub(crate) fn lock_key(source: Source) -> String {
    AcquireLock::key("library", "reconcile", 1, [source.name().to_owned()])
}

pub(crate) async fn remove(
    db_context: &mut DbContext,
    source: Source,
    ids: Vec<Uuid>,
    listed: &HashSet<i64>,
) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    match source {
        Source::Radarr => {
            let movies = db_context
                .movies()
                .filter(Movie::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .into_iter()
                .filter(|x| !listed.contains(&x.tmdb_id))
                .collect();
            movies::remove(db_context, movies).await
        }
        Source::Sonarr => {
            let shows = db_context
                .shows()
                .filter(Show::fields().id().in_list(ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
            for show in shows.into_iter().filter(|x| !listed.contains(&x.tvdb_id)) {
                shows::remove(db_context, show).await?;
            }
            Ok(())
        }
    }
}
