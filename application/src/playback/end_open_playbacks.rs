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
    db_context
        .playbacks()
        .filter(Playback::open_filter())
        .update()
        .ended_at(Some(clock.call()))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;

    const NOW: &str = "2026-11-03T20:00:00Z";

    #[tokio::test]
    async fn every_open_playback_ends_and_an_ended_one_keeps_its_end_time() {
        let mut db_context = mock_db_context().await;
        let reported_at = at("2026-11-03T19:59:50Z");
        let stopped_at = at("2026-11-03T19:00:00Z");
        let open = playback(&db_context, reported_at)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let stopped = playback(&db_context, stopped_at)
            .ended_at(Some(stopped_at))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(|| at(NOW)),
        })
        .await
        .unwrap();

        let mut ended_at = Vec::new();
        for id in [open, stopped] {
            ended_at.push(
                db_context
                    .playbacks()
                    .find(id)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap()
                    .unwrap()
                    .ended_at,
            );
        }
        assert_eq!(ended_at, [Some(at(NOW)), Some(stopped_at)]);
    }
}
