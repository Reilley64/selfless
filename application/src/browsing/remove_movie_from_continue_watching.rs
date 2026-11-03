mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Movie;
use domain::MovieContinueWatchingRemoval;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    movie_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let movie = db_context
        .movies()
        .filter(
            Movie::fields()
                .id()
                .eq(movie_id)
                .and(Movie::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if movie.is_none() {
        return Ok(Output::NotFound);
    }
    MovieContinueWatchingRemoval::upsert_by_user_id_and_movie_id(user_id, movie_id)
        .removed_at(clock.call())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browsing::get_movie;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::add_media_file;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::movie;
    use crate::test::stop_at;
    use crate::test::url_signer;
    use jiff::SignedDuration;
    use test_fixtures::FakeClock;

    async fn remove(db_context: &DbContext, user_id: Uuid, movie_id: Uuid, now: &str) -> Output {
        let clock = FakeClock::new();
        clock.set(at(now));
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            user_id,
            movie_id,
        )
        .await
        .unwrap()
    }

    async fn removals(db_context: &DbContext) -> Vec<(Uuid, Uuid, jiff::Timestamp)> {
        let mut db_context = db_context.clone();
        db_context
            .movie_continue_watching_removals()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.user_id, x.movie_id, x.removed_at))
            .collect()
    }

    #[tokio::test]
    async fn a_removal_stores_the_time_for_the_user_and_movie_and_a_second_removal_updates_it() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let arrival = movie(&db_context, "Arrival")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        let first = remove(&db_context, noor, arrival, "2026-11-01T20:00:00Z").await;
        let second = remove(&db_context, noor, arrival, "2026-11-02T20:00:00Z").await;

        assert_eq!((first, second), (Output::Removed, Output::Removed));
        assert_eq!(
            removals(&db_context).await,
            [(noor, arrival, at("2026-11-02T20:00:00Z"))]
        );
    }

    #[tokio::test]
    async fn an_unknown_or_hidden_movie_is_not_found() {
        let mut db_context = mock_db_context().await;
        let gone = movie(&db_context, "Gone")
            .in_library(false)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        for movie_id in [gone, Uuid::new_v4()] {
            let output = remove(
                &db_context,
                Uuid::new_v4(),
                movie_id,
                "2026-11-01T20:00:00Z",
            )
            .await;

            assert_eq!(output, Output::NotFound);
        }
        assert_eq!(removals(&db_context).await, []);
    }

    #[tokio::test]
    async fn the_movie_page_still_offers_resume_after_a_removal() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let media_file_id = add_media_file(&db_context, Some(3_000_000)).await;
        let arrival = movie(&db_context, "Arrival")
            .media_file_id(Some(media_file_id))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        stop_at(
            &db_context,
            noor,
            media_file_id,
            SignedDuration::from_mins(10),
            "2026-11-01T20:00:00Z",
        )
        .await;

        remove(&db_context, noor, arrival, "2026-11-02T20:00:00Z").await;

        let clock = FakeClock::new();
        let get_movie::Output::Movie(page) = get_movie::call(
            get_movie::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
                url_signer: url_signer(),
            },
            noor,
            arrival,
        )
        .await
        .unwrap() else {
            panic!("Arrival is not found");
        };
        assert_eq!(page.resume_position_seconds, Some(600));
    }
}
