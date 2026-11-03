pub(crate) mod movies;
pub(crate) mod shows;

use crate::ports::AcquireLock;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbScope;
use domain::Movie;
use domain::Show;
use domain::Source;
use domain::TmdbId;
use domain::TvdbId;
use std::collections::HashSet;
use uuid::Uuid;

#[ohno::error]
#[display("failed to sync the library")]
pub(crate) struct Error;

pub(crate) fn lock_key(source: Source) -> String {
    AcquireLock::key("library", "reconcile", 1, [source.name().to_owned()])
}

pub(crate) enum Removal {
    Movies {
        movie_ids: Vec<Uuid>,
        listed: HashSet<TmdbId>,
    },
    Shows {
        show_ids: Vec<Uuid>,
        listed: HashSet<TvdbId>,
    },
}

impl Removal {
    pub(crate) fn count(&self) -> usize {
        match self {
            Removal::Movies { movie_ids, .. } => movie_ids.len(),
            Removal::Shows { show_ids, .. } => show_ids.len(),
        }
    }
}

pub(crate) async fn remove(db_context: &mut DbContext, removal: Removal) -> Result<(), Error> {
    if removal.count() == 0 {
        return Ok(());
    }
    match removal {
        Removal::Movies { movie_ids, listed } => {
            let movies = db_context
                .movies()
                .filter(Movie::fields().id().in_list(movie_ids))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .into_iter()
                .filter(|x| !listed.contains(&x.tmdb_id))
                .collect();
            movies::remove(db_context, movies).await
        }
        Removal::Shows { show_ids, listed } => {
            let shows = db_context
                .shows()
                .filter(Show::fields().id().in_list(show_ids))
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
