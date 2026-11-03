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
        .include(HeldRemoval::fields().items())
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
    let item_ids = held_removal.items.get().iter().map(|x| x.item_id).collect();
    reconcile::remove(&mut db_context, source, item_ids, &listed)
        .await
        .map_err(Error::caused_by)?;
    held_removal
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Reconciles;
    use crate::library::SourceHealth;
    use crate::ports::ListRadarrMovies;
    use crate::ports::ListSonarrShows;
    use crate::ports::RadarrMovie;
    use crate::ports::persistence::DbContext;
    use crate::test::arrival;
    use crate::test::mock_db_context;
    use domain::Movie;
    use std::sync::Arc;
    use std::sync::Mutex;

    const ARRIVAL: i64 = 1;
    const DUNE: i64 = 2;
    const SICARIO: i64 = 3;

    struct Library {
        db_context: DbContext,
        radarr: Arc<Mutex<Vec<i64>>>,
    }

    impl Library {
        async fn new(titles: &[(i64, &str)]) -> Self {
            let mut db_context = mock_db_context().await;
            for (tmdb_id, title) in titles {
                db_context
                    .movies()
                    .create()
                    .tmdb_id(*tmdb_id)
                    .radarr_id(*tmdb_id)
                    .title(*title)
                    .year(2016)
                    .genres(Vec::<String>::new())
                    .directors(Vec::<String>::new())
                    .visible(true)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap();
            }
            Self {
                db_context,
                radarr: Arc::default(),
            }
        }

        async fn hold(&self, tmdb_ids: &[i64]) {
            let mut db_context = self.db_context.clone();
            db_context
                .held_removals()
                .create()
                .source(Source::Radarr)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            let movies = db_context
                .movies()
                .filter(Movie::fields().tmdb_id().in_list(tmdb_ids.to_vec()))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
            for movie in movies {
                db_context
                    .held_removal_items()
                    .create()
                    .source(Source::Radarr)
                    .item_id(movie.id)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap();
            }
        }

        fn radarr_lists(&self, tmdb_ids: &[i64]) {
            *self.radarr.lock().unwrap() = tmdb_ids.to_vec();
        }

        async fn confirm(&self) {
            let radarr = self.radarr.clone();
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    list_radarr_movies: ListRadarrMovies::new(move || {
                        let movies = radarr
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|x| RadarrMovie {
                                tmdb_id: *x,
                                ..arrival(None)
                            })
                            .collect();
                        Box::pin(async move { Ok(movies) })
                    }),
                    list_sonarr_shows: ListSonarrShows::new(|| Box::pin(async { Ok(Vec::new()) })),
                    reconciles: Reconciles::default(),
                    source_health: SourceHealth::default(),
                },
                Source::Radarr,
            )
            .await
            .unwrap();
        }

        async fn movie_titles(&self) -> Vec<String> {
            let mut db_context = self.db_context.clone();
            let mut titles = db_context
                .movies()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .into_iter()
                .map(|x| x.title)
                .collect::<Vec<_>>();
            titles.sort();
            titles
        }

        async fn is_held(&self) -> bool {
            let mut db_context = self.db_context.clone();
            db_context
                .held_removals()
                .all()
                .first()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .is_some()
        }
    }

    #[tokio::test]
    async fn confirming_a_held_removal_removes_the_held_movies_and_clears_the_hold() {
        let library =
            Library::new(&[(ARRIVAL, "Arrival"), (DUNE, "Dune"), (SICARIO, "Sicario")]).await;
        library.hold(&[DUNE, SICARIO]).await;
        library.radarr_lists(&[ARRIVAL]);

        library.confirm().await;

        assert_eq!(library.movie_titles().await, ["Arrival"]);
        assert!(!library.is_held().await);
    }

    #[tokio::test]
    async fn confirming_a_held_removal_keeps_the_movies_radarr_lists_again() {
        let library =
            Library::new(&[(ARRIVAL, "Arrival"), (DUNE, "Dune"), (SICARIO, "Sicario")]).await;
        library.hold(&[DUNE, SICARIO]).await;
        library.radarr_lists(&[ARRIVAL, DUNE]);

        library.confirm().await;

        assert_eq!(library.movie_titles().await, ["Arrival", "Dune"]);
        assert!(!library.is_held().await);
    }
}
