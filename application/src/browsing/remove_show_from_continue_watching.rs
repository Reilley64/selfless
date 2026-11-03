mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Show;
use domain::ShowContinueWatchingRemoval;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    show_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let show = db_context
        .shows()
        .filter(
            Show::fields()
                .id()
                .eq(show_id)
                .and(Show::in_library_filter()),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if show.is_none() {
        return Ok(Output::NotFound);
    }
    ShowContinueWatchingRemoval::upsert_by_user_id_and_show_id(user_id, show_id)
        .removed_at(clock.call())
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::add_season;
    use crate::test::at;
    use crate::test::episode;
    use crate::test::mock_db_context;
    use crate::test::show;
    use test_fixtures::FakeClock;

    async fn remove(db_context: &DbContext, user_id: Uuid, show_id: Uuid, now: &str) -> Output {
        let clock = FakeClock::new();
        clock.set(at(now));
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            user_id,
            show_id,
        )
        .await
        .unwrap()
    }

    async fn add_show(db_context: &mut DbContext, title: &str, in_library: bool) -> Uuid {
        let show_id = show(db_context, title)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let season_id = add_season(db_context, show_id, 1).await;
        episode(db_context, season_id)
            .in_library(in_library)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        show_id
    }

    async fn removals(db_context: &DbContext) -> Vec<(Uuid, Uuid, jiff::Timestamp)> {
        let mut db_context = db_context.clone();
        db_context
            .show_continue_watching_removals()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.user_id, x.show_id, x.removed_at))
            .collect()
    }

    #[tokio::test]
    async fn a_removal_stores_the_time_for_the_user_and_show_and_a_second_removal_updates_it() {
        let mut db_context = mock_db_context().await;
        let noor = Uuid::new_v4();
        let severance = add_show(&mut db_context, "Severance", true).await;

        let first = remove(&db_context, noor, severance, "2026-11-01T20:00:00Z").await;
        let second = remove(&db_context, noor, severance, "2026-11-02T20:00:00Z").await;

        assert_eq!((first, second), (Output::Removed, Output::Removed));
        assert_eq!(
            removals(&db_context).await,
            [(noor, severance, at("2026-11-02T20:00:00Z"))]
        );
    }

    #[tokio::test]
    async fn an_unknown_show_or_one_with_no_episode_in_the_library_is_not_found() {
        let mut db_context = mock_db_context().await;
        let gone = add_show(&mut db_context, "Gone", false).await;

        for show_id in [gone, Uuid::new_v4()] {
            let output = remove(&db_context, Uuid::new_v4(), show_id, "2026-11-01T20:00:00Z").await;

            assert_eq!(output, Output::NotFound);
        }
        assert_eq!(removals(&db_context).await, []);
    }
}
