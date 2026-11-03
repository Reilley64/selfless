mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Playback;
use domain::PlaybackReason;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let expired = transaction
        .playbacks()
        .filter(Playback::expired_filter(clock.call()))
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .map(|x| x.id)
        .collect::<Vec<_>>();
    transaction
        .playback_reasons()
        .filter(
            PlaybackReason::fields()
                .playback_id()
                .in_list(expired.clone()),
        )
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .playbacks()
        .filter(Playback::fields().id().in_list(expired))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::test::add_playback_reason;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;
    use crate::test::playback_reason_owners;

    #[tokio::test]
    async fn playbacks_that_ended_30_days_ago_are_deleted_with_their_reasons() {
        let mut db_context = mock_db_context().await;
        let started_at = at("2026-05-01T10:00:00Z");
        let mut kept = Vec::new();
        for ended_at in [
            Some(at("2026-05-02T12:00:00Z")),
            Some(at("2026-05-02T12:00:01Z")),
            None,
        ] {
            let id = playback(&db_context, started_at)
                .ended_at(ended_at)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .id;
            add_playback_reason(&mut db_context, id).await;
            kept.push(id);
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
        assert_eq!(playback_reason_owners(&mut db_context).await, kept);
    }
}
