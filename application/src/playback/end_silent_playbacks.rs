mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Playback;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    db_context
        .playbacks()
        .filter(Playback::silent_filter(now))
        .update()
        .ended_at(Some(now))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;
    use jiff::Timestamp;
    use uuid::Uuid;

    const NOW: &str = "2026-06-01T12:00:00Z";

    async fn end_silent_playbacks(db_context: &DbContext) {
        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(|| at(NOW)),
        })
        .await
        .unwrap();
    }

    async fn ended_at(db_context: &mut DbContext, id: Uuid) -> Option<Timestamp> {
        db_context
            .playbacks()
            .find(id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .ended_at
    }

    #[tokio::test]
    async fn a_playback_with_no_report_for_60_seconds_ends() {
        let mut db_context = mock_db_context().await;
        let silent = playback(&db_context, at("2026-06-01T11:59:00Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let reporting = playback(&db_context, at("2026-06-01T11:59:01Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        end_silent_playbacks(&db_context).await;

        assert_eq!(ended_at(&mut db_context, silent).await, Some(at(NOW)));
        assert_eq!(ended_at(&mut db_context, reporting).await, None);
    }

    #[tokio::test]
    async fn a_playback_that_already_ended_keeps_its_end_time() {
        let mut db_context = mock_db_context().await;
        let stopped_at = at("2026-06-01T11:55:00Z");
        let stopped = playback(&db_context, stopped_at)
            .ended_at(Some(stopped_at))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        end_silent_playbacks(&db_context).await;

        assert_eq!(ended_at(&mut db_context, stopped).await, Some(stopped_at));
    }
}
