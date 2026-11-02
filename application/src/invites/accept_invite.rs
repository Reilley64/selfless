mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::InviteNotFound;
pub use output::Output;

use crate::ports::AcquireLock;
use crate::ports::persistence::DbScope;
use crate::tokens;
use crate::users::create_user;
use domain::Invite;
use domain::Password;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
    name: String,
    password: Password,
    idempotency_key: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        acquire_lock,
    } = dependencies.into();
    let _guard = acquire_lock
        .call(AcquireLock::key(
            "invites",
            "accept",
            1,
            [idempotency_key.to_string()],
        ))
        .await;
    if let Some(user) = db_context
        .users()
        .filter(User::fields().name().eq(&name))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| x.idempotency_key == Some(idempotency_key))
    {
        return Ok(Output {
            id: user.id,
            colour: user.colour(),
            name: user.name,
        });
    }
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let Some(invite) = transaction
        .invites()
        .filter(Invite::fields().token_hash().eq(tokens::hash(token)))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| !x.expired(now))
    else {
        return Err(Error::caused_by(InviteNotFound::new()));
    };
    let user = match create_user::create(
        &mut transaction,
        name,
        password,
        invite.email.clone(),
        Some(idempotency_key),
    )
    .await
    {
        Ok(user) => user,
        Err(error) => {
            drop(transaction);
            let accepted = db_context
                .users()
                .filter(User::fields().email().eq(invite.email))
                .first()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .is_some();
            return Err(if accepted {
                Error::caused_by(InviteNotFound::new())
            } else {
                Error::caused_by(error)
            });
        }
    };
    invite
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output {
        id: user.id,
        colour: user.colour(),
        name: user.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::mock_db_context;
    use domain::EmailAddress;
    use domain::InviteEmailStatus;
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    const TOKEN: &str = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M";

    async fn sent_invite(db_context: &mut DbContext, clock: &FakeClock) {
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
            .unwrap();
    }

    async fn accept(
        db_context: &DbContext,
        clock: &FakeClock,
        token: &str,
        name: &str,
        idempotency_key: Uuid,
    ) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
                acquire_lock: RecordingLock::default().port(),
            },
            token,
            name.to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            idempotency_key,
        )
        .await
    }

    #[tokio::test]
    async fn a_link_works_once() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        sent_invite(&mut db_context, &clock).await;

        let first = accept(&db_context, &clock, TOKEN, "Noor", Uuid::new_v4()).await;
        let second = accept(&db_context, &clock, TOKEN, "Ravi", Uuid::new_v4()).await;

        assert!(first.is_ok(), "{first:?}");
        let error = second.expect_err("the link works once");
        assert!(error.find_source::<InviteNotFound>().is_some(), "{error}");
    }

    #[tokio::test]
    async fn a_taken_name_returns_409_and_leaves_the_invite_usable() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        db_context
            .users()
            .create()
            .name("Noor")
            .email(EmailAddress::new("noor@example.com".to_owned()).unwrap())
            .password_hash("")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        sent_invite(&mut db_context, &clock).await;

        let taken = accept(&db_context, &clock, TOKEN, "Noor", Uuid::new_v4()).await;
        let other = accept(&db_context, &clock, TOKEN, "Ravi", Uuid::new_v4()).await;

        let error = taken.expect_err("the name is taken");
        assert!(
            error.find_source::<create_user::NameTaken>().is_some(),
            "{error}"
        );
        assert_eq!(other.unwrap().name, "Ravi");
    }

    #[tokio::test]
    async fn repeating_an_accept_with_the_same_key_returns_the_same_user() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        sent_invite(&mut db_context, &clock).await;
        let key = Uuid::new_v4();

        let first = accept(&db_context, &clock, TOKEN, "Noor", key)
            .await
            .unwrap();
        let repeat = accept(&db_context, &clock, TOKEN, "Noor", key)
            .await
            .unwrap();

        assert_eq!(repeat.id, first.id);
        assert_eq!(repeat.name, first.name);
    }
}
