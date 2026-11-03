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
        .filter(Playback::expired_filter(clock.call()))
        .delete()
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

    #[tokio::test]
    async fn playbacks_that_ended_30_days_ago_are_deleted() {
        let mut db_context = mock_db_context().await;
        let started_at = at("2026-05-01T10:00:00Z");
        let mut kept = Vec::new();
        for ended_at in [
            Some(at("2026-05-02T12:00:00Z")),
            Some(at("2026-05-02T12:00:01Z")),
            None,
        ] {
            kept.push(
                playback(&db_context, started_at)
                    .ended_at(ended_at)
                    .exec(db_context.executor_mut())
                    .await
                    .unwrap()
                    .id,
            );
        }
        kept.remove(0);

        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(|| at("2026-06-01T12:00:00Z")),
        })
        .await
        .unwrap();

        let mut remaining = db_context
            .playbacks()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect::<Vec<_>>();
        remaining.sort();
        kept.sort();
        assert_eq!(remaining, kept);
    }
}
