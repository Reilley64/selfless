mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::InviteNotFound;
pub use output::Output;

use crate::invites::invite_email;
use crate::ports::persistence::DbScope;
use domain::InviteEmailStatus;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    origin: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        acquire_lock,
    } = dependencies.into();
    let _guard = acquire_lock.call(invite_email::lock_key(id)).await;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let Some(mut invite) = transaction
        .invites()
        .find(id)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Err(Error::caused_by(InviteNotFound::new()));
    };
    invite
        .update()
        .email_status(InviteEmailStatus::Queued {
            queued_at: clock.call(),
        })
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    invite_email::queue(&mut transaction, invite.id, origin)
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invites::create_invite;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::mock_db_context;
    use domain::EmailAddress;
    use domain::Invite;
    use domain::Outbox;
    use domain::OutboxData;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    const ORIGIN: &str = "https://selfless.example";

    async fn queue_invite(db_context: &DbContext, clock: &FakeClock) -> Invite {
        create_invite::call(
            create_invite::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            Uuid::nil(),
            EmailAddress::new("noor@household.example".to_owned()).unwrap(),
            ORIGIN.to_owned(),
        )
        .await
        .unwrap();
        let mut db_context = db_context.clone();
        db_context
            .invites()
            .all()
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
    }

    async fn resend(
        db_context: &DbContext,
        clock: &FakeClock,
        lock: &RecordingLock,
        id: Uuid,
    ) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
                acquire_lock: lock.port(),
            },
            id,
            ORIGIN.to_owned(),
        )
        .await
    }

    async fn find(db_context: &DbContext, id: Uuid) -> Invite {
        let mut db_context = db_context.clone();
        db_context
            .invites()
            .find(id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
    }

    async fn outboxes(db_context: &DbContext) -> Vec<Outbox> {
        let mut db_context = db_context.clone();
        db_context
            .outboxes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_resend_queues_a_new_email_and_keeps_the_old_link() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let mut invite = queue_invite(&db_context, &clock).await;
        let expires_at = Timestamp::from_second(1_800_000_000).unwrap();
        invite
            .update()
            .token_hash(Some(b"the old link".to_vec()))
            .email_status(InviteEmailStatus::Sent {
                sent_at: clock.now(),
            })
            .expires_at(Some(expires_at))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        clock.advance(SignedDuration::from_hours(1));

        resend(&db_context, &clock, &RecordingLock::default(), invite.id)
            .await
            .unwrap();

        let resent = find(&db_context, invite.id).await;
        assert_eq!(
            resent.email_status,
            InviteEmailStatus::Queued {
                queued_at: clock.now()
            }
        );
        assert_eq!(resent.token_hash, Some(b"the old link".to_vec()));
        assert_eq!(resent.expires_at, Some(expires_at));
        let outboxes = outboxes(&db_context).await;
        assert_eq!(outboxes.len(), 2);
        for outbox in outboxes {
            assert_eq!(
                outbox.data,
                OutboxData::SendInviteEmail {
                    invite_id: invite.id,
                    origin: ORIGIN.to_owned(),
                }
            );
            assert_eq!(outbox.delivered_at, None);
        }
    }

    #[tokio::test]
    async fn resending_an_invite_that_is_no_longer_pending_fails_with_invite_not_found() {
        let db_context = mock_db_context().await;

        let result = resend(
            &db_context,
            &FakeClock::new(),
            &RecordingLock::default(),
            Uuid::new_v4(),
        )
        .await;

        let error = result.expect_err("no invite has this ID");
        assert!(error.find_source::<InviteNotFound>().is_some(), "{error}");
        assert_eq!(outboxes(&db_context).await.len(), 0);
    }

    #[tokio::test]
    async fn a_rejected_invite_can_be_resent() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let mut invite = queue_invite(&db_context, &clock).await;
        invite
            .update()
            .email_status(InviteEmailStatus::Rejected {
                rejected_at: clock.now(),
            })
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        clock.advance(SignedDuration::from_hours(1));

        resend(&db_context, &clock, &RecordingLock::default(), invite.id)
            .await
            .unwrap();

        assert_eq!(
            find(&db_context, invite.id).await.email_status,
            InviteEmailStatus::Queued {
                queued_at: clock.now()
            }
        );
        assert_eq!(outboxes(&db_context).await.len(), 2);
    }

    #[tokio::test]
    async fn resending_takes_the_invite_email_lock() {
        let db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let lock = RecordingLock::default();
        let invite = queue_invite(&db_context, &clock).await;

        resend(&db_context, &clock, &lock, invite.id).await.unwrap();

        assert_eq!(lock.keys(), [format!("invites:email:v1:{}", invite.id)]);
    }
}
