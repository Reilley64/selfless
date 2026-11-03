mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::invites::invite_email;
use crate::ports::persistence::DbScope;
use domain::Invite;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        acquire_lock,
    } = dependencies.into();
    let _guard = acquire_lock.call(invite_email::lock_key(id)).await;
    db_context
        .invites()
        .filter(Invite::fields().id().eq(id))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invites::accept_invite;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::mock_db_context;
    use crate::tokens;
    use domain::EmailAddress;
    use domain::InviteEmailStatus;
    use domain::Password;
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    const TOKEN: &str = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M";

    async fn sent_invite(db_context: &mut DbContext, clock: &FakeClock) -> Uuid {
        db_context
            .invites()
            .create()
            .email(EmailAddress::new("noor@household.example".to_owned()).unwrap())
            .token_hash(Some(tokens::hash(TOKEN)))
            .admin_id(Uuid::nil())
            .email_status(InviteEmailStatus::Sent {
                sent_at: clock.now(),
            })
            .expires_at(Some(Invite::expires_at(clock.now())))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn cancel(db_context: &DbContext, lock: &RecordingLock, id: Uuid) {
        call(
            Dependencies {
                db_context: db_context.clone(),
                acquire_lock: lock.port(),
            },
            id,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_cancelled_invite_is_no_longer_pending_and_its_link_stops_working() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let id = sent_invite(&mut db_context, &clock).await;

        cancel(&db_context, &RecordingLock::default(), id).await;

        let pending = db_context
            .invites()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        assert!(pending.is_empty(), "{pending:?}");
        let error = accept_invite::call(
            accept_invite::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
                acquire_lock: RecordingLock::default().port(),
            },
            TOKEN,
            "Noor".to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            Uuid::new_v4(),
        )
        .await
        .expect_err("the link no longer works");
        assert!(
            error
                .find_source::<accept_invite::InviteNotFound>()
                .is_some(),
            "{error}"
        );
    }

    #[tokio::test]
    async fn cancelling_takes_the_invite_email_lock() {
        let mut db_context = mock_db_context().await;
        let lock = RecordingLock::default();
        let id = sent_invite(&mut db_context, &FakeClock::new()).await;

        cancel(&db_context, &lock, id).await;

        assert_eq!(lock.keys(), [format!("invites:email:v1:{id}")]);
    }
}
