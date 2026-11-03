mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Inbox;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        clock,
        mut db_context,
    } = dependencies.into();
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let deleted = transaction
        .inboxes()
        .filter(Inbox::expired_filter(now))
        .count()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .inboxes()
        .filter(Inbox::expired_filter(now))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output {
        deleted: i64::try_from(deleted).map_err(Error::caused_by)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::test::mock_db_context;
    use domain::Source;
    use jiff::SignedDuration;
    use test_fixtures::FakeClock;

    #[tokio::test]
    async fn delivered_deliveries_are_deleted_30_days_after_they_were_received() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let received_at = clock.now();
        for (body, delivered_at) in [("delivered", Some(received_at)), ("undelivered", None)] {
            db_context
                .inboxes()
                .create()
                .source(Source::Radarr)
                .body(body.as_bytes().to_vec())
                .received_at(received_at)
                .delivered_at(delivered_at)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let dependencies = || Dependencies {
            clock: Clock::new(clock.reader()),
            db_context: db_context.clone(),
        };

        clock.advance(SignedDuration::from_hours(30 * 24) - SignedDuration::from_secs(1));
        let early = call(dependencies()).await.unwrap();
        clock.advance(SignedDuration::from_secs(1));
        let due = call(dependencies()).await.unwrap();

        let kept = db_context
            .inboxes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.body)
            .collect::<Vec<_>>();
        assert_eq!(early.deleted, 0);
        assert_eq!(due.deleted, 1);
        assert_eq!(kept, [b"undelivered".to_vec()]);
    }
}
