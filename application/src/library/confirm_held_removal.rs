mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::library::sync;
use crate::library::sync::Removal;
use crate::ports::persistence::DbScope;
use domain::HeldRemoval;
use domain::HeldRemovalItem;
use domain::Owner;
use domain::Source;

pub async fn call(dependencies: impl Into<Dependencies>, source: Source) -> Result<Output, Error> {
    let Dependencies {
        acquire_lock,
        mut db_context,
        list_radarr_movies,
        list_sonarr_shows,
        source_health,
    } = dependencies.into();
    let _running = acquire_lock.call(sync::lock_key(source)).await;
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
    let items = held_removal.items.get();
    let removal = match source {
        Source::Radarr => Removal::Movies {
            movie_ids: items
                .iter()
                .filter_map(HeldRemovalItem::owner)
                .filter_map(Owner::movie_id)
                .collect(),
            listed: source_health
                .api_call(source, list_radarr_movies.call().await)
                .map_err(Error::caused_by)?
                .into_iter()
                .map(|x| x.tmdb_id)
                .collect(),
        },
        Source::Sonarr => Removal::Shows {
            show_ids: items
                .iter()
                .filter_map(HeldRemovalItem::owner)
                .filter_map(Owner::show_id)
                .collect(),
            listed: source_health
                .api_call(source, list_sonarr_shows.call().await)
                .map_err(Error::caused_by)?
                .into_iter()
                .map(|x| x.tvdb_id)
                .collect(),
        },
    };
    sync::remove(&mut db_context, removal)
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
    use crate::library::SourceHealth;
    use crate::ports::ListRadarrMovies;
    use crate::ports::ListSonarrShows;
    use crate::ports::SourceMovie;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::arrival;
    use crate::test::mock_db_context;
    use crate::test::movie_titles;
    use domain::RadarrId;
    use domain::TmdbId;
    use std::sync::Arc;
    use std::sync::Mutex;

    const ARRIVAL: TmdbId = TmdbId(1);
    const DUNE: TmdbId = TmdbId(2);
    const SICARIO: TmdbId = TmdbId(3);

    struct Library {
        db_context: DbContext,
        radarr: Arc<Mutex<Vec<TmdbId>>>,
        lock: RecordingLock,
    }

    impl Library {
        async fn new(titles: &[(TmdbId, &str)]) -> Self {
            let mut db_context = mock_db_context().await;
            for (tmdb_id, title) in titles {
                db_context
                    .movies()
                    .create()
                    .tmdb_id(*tmdb_id)
                    .radarr_id(RadarrId(tmdb_id.0 + 1_000))
                    .title(*title)
                    .year(2016)
                    .genres(Vec::<String>::new())
                    .directors(Vec::<String>::new())
                    .in_library(true)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap();
            }
            Self {
                db_context,
                radarr: Arc::default(),
                lock: RecordingLock::default(),
            }
        }

        async fn hold(&self, tmdb_ids: &[TmdbId]) {
            let mut db_context = self.db_context.clone();
            let items = db_context
                .movies()
                .all()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .into_iter()
                .filter(|x| tmdb_ids.contains(&x.tmdb_id))
                .map(|x| HeldRemovalItem::create().movie_id(Some(x.id)))
                .collect::<Vec<_>>();
            db_context
                .held_removals()
                .create()
                .source(Source::Radarr)
                .items(items)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        fn radarr_lists(&self, tmdb_ids: &[TmdbId]) {
            *self.radarr.lock().unwrap() = tmdb_ids.to_vec();
        }

        async fn confirm(&self) {
            let radarr = self.radarr.clone();
            call(
                Dependencies {
                    acquire_lock: self.lock.port(),
                    db_context: self.db_context.clone(),
                    list_radarr_movies: ListRadarrMovies::new(move || {
                        let movies = radarr
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|x| SourceMovie {
                                tmdb_id: *x,
                                ..arrival(None)
                            })
                            .collect();
                        Box::pin(async move { Ok(movies) })
                    }),
                    list_sonarr_shows: ListSonarrShows::new(|| Box::pin(async { Ok(Vec::new()) })),
                    source_health: SourceHealth::default(),
                },
                Source::Radarr,
            )
            .await
            .unwrap();
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

        async fn held_item_count(&self) -> usize {
            let mut db_context = self.db_context.clone();
            toasty::sql::query("SELECT 1 FROM held_removal_items")
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .len()
        }
    }

    #[tokio::test]
    async fn confirming_a_held_removal_waits_for_the_reconcile_lock_of_its_source() {
        let library = Library::new(&[(ARRIVAL, "Arrival"), (DUNE, "Dune")]).await;
        library.hold(&[DUNE]).await;
        library.radarr_lists(&[ARRIVAL]);

        library.confirm().await;

        assert_eq!(library.lock.keys(), [sync::lock_key(Source::Radarr)]);
    }

    #[tokio::test]
    async fn confirming_a_held_removal_removes_the_held_movies_and_clears_the_hold() {
        let library =
            Library::new(&[(ARRIVAL, "Arrival"), (DUNE, "Dune"), (SICARIO, "Sicario")]).await;
        library.hold(&[DUNE, SICARIO]).await;
        library.radarr_lists(&[ARRIVAL]);

        library.confirm().await;

        assert_eq!(movie_titles(&library.db_context).await, ["Arrival"]);
        assert!(!library.is_held().await);
        assert_eq!(library.held_item_count().await, 0);
    }

    #[tokio::test]
    async fn confirming_a_held_removal_keeps_the_movies_radarr_lists_again() {
        let library =
            Library::new(&[(ARRIVAL, "Arrival"), (DUNE, "Dune"), (SICARIO, "Sicario")]).await;
        library.hold(&[DUNE, SICARIO]).await;
        library.radarr_lists(&[ARRIVAL, DUNE]);

        library.confirm().await;

        assert_eq!(movie_titles(&library.db_context).await, ["Arrival", "Dune"]);
        assert!(!library.is_held().await);
        assert_eq!(library.held_item_count().await, 0);
    }
}
