mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::invites::invite_email;
use crate::ports::EmailRejected;
use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Invite;
use domain::InviteEmailStatus;
use ohno::ErrorExt;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    invite_id: Uuid,
    origin: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        acquire_lock,
        send_email,
    } = dependencies.into();
    let _guard = acquire_lock.call(invite_email::lock_key(invite_id)).await;
    let Some(mut invite) = db_context
        .invites()
        .find(invite_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output);
    };
    let InviteEmailStatus::Queued { .. } = invite.email_status else {
        return Ok(Output);
    };
    let token = tokens::generate();
    match invite_email::send(&send_email, invite.email.clone(), &origin, &token).await {
        Ok(()) => {
            let sent_at = clock.call();
            invite
                .update()
                .token_hash(Some(tokens::hash(&token)))
                .email_status(InviteEmailStatus::Sent { sent_at })
                .expires_at(Some(Invite::expires_at(sent_at)))
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
        Err(error) if error.find_source::<EmailRejected>().is_some() => {
            tracing::warn!(
                error = &error as &dyn std::error::Error,
                "the mail server refused an invite email"
            );
            invite
                .update()
                .email_status(InviteEmailStatus::Rejected {
                    rejected_at: clock.call(),
                })
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
        Err(error) => return Err(Error::caused_by(error)),
    }
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invites::accept_invite;
    use crate::invites::cancel_invite;
    use crate::invites::create_invite;
    use crate::invites::resend_invite;
    use crate::ports::Clock;
    use crate::ports::SendEmail;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::failing_mailer;
    use crate::test::mock_db_context;
    use crate::test::refusing_mailer;
    use domain::EmailAddress;
    use domain::Password;
    use jiff::SignedDuration;
    use std::io;
    use std::sync::Arc;
    use std::sync::Mutex;
    use test_fixtures::FakeClock;
    use test_fixtures::FakeMailer;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const ORIGIN: &str = "https://selfless.example";
    const EMAIL: &str = "noor@household.example";

    struct Invites {
        db_context: DbContext,
        clock: FakeClock,
        mailer: FakeMailer,
        lock: RecordingLock,
    }

    impl Invites {
        async fn new() -> Self {
            Self {
                db_context: mock_db_context().await,
                clock: FakeClock::new(),
                mailer: FakeMailer::default(),
                lock: RecordingLock::default(),
            }
        }

        async fn queue(&self) -> Uuid {
            create_invite::call(
                create_invite::Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                },
                Uuid::nil(),
                EmailAddress::new(EMAIL.to_owned()).unwrap(),
                ORIGIN.to_owned(),
            )
            .await
            .unwrap();
            let mut db_context = self.db_context.clone();
            db_context
                .invites()
                .all()
                .first()
                .exec(db_context.executor_mut())
                .await
                .unwrap()
                .unwrap()
                .id
        }

        async fn send(&self, invite_id: Uuid) -> Result<Output, Error> {
            self.send_with(invite_id, SendEmail::new(self.mailer.sender()))
                .await
        }

        async fn send_with(&self, invite_id: Uuid, send_email: SendEmail) -> Result<Output, Error> {
            call(
                Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                    acquire_lock: self.lock.port(),
                    send_email,
                },
                invite_id,
                ORIGIN.to_owned(),
            )
            .await
        }

        async fn resend(&self, invite_id: Uuid) {
            resend_invite::call(
                resend_invite::Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                    acquire_lock: self.lock.port(),
                },
                invite_id,
                ORIGIN.to_owned(),
            )
            .await
            .unwrap();
        }

        async fn invite(&self, invite_id: Uuid) -> Option<Invite> {
            let mut db_context = self.db_context.clone();
            db_context
                .invites()
                .find(invite_id)
                .exec(db_context.executor_mut())
                .await
                .unwrap()
        }

        async fn accept(&self, token: &str) -> Result<accept_invite::Output, accept_invite::Error> {
            accept_invite::call(
                accept_invite::Dependencies {
                    db_context: self.db_context.clone(),
                    clock: Clock::new(self.clock.reader()),
                    acquire_lock: RecordingLock::default().port(),
                },
                token,
                "Noor".to_owned(),
                Password::new("a long passphrase".to_owned()).unwrap(),
                Uuid::new_v4(),
            )
            .await
        }

        fn last_token(&self) -> String {
            let body = self.mailer.sent().last().unwrap().body.clone();
            body.split(&format!("{ORIGIN}/invite#"))
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .to_owned()
        }
    }

    #[derive(Clone, Default)]
    struct Logs(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Logs {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Logs {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    #[tokio::test]
    async fn the_email_holds_the_link_whose_hash_the_invite_stores() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;

        invites.send(invite_id).await.unwrap();

        let sent = invites.mailer.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].recipient, EMAIL);
        let invite = invites.invite(invite_id).await.unwrap();
        assert_eq!(invite.token_hash, Some(tokens::hash(&invites.last_token())));
    }

    #[tokio::test]
    async fn the_7_days_start_when_the_email_is_sent() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        invites.clock.advance(SignedDuration::from_hours(3));

        invites.send(invite_id).await.unwrap();

        let sent_at = invites.clock.now();
        let invite = invites.invite(invite_id).await.unwrap();
        assert_eq!(invite.email_status, InviteEmailStatus::Sent { sent_at });
        assert_eq!(
            invite.expires_at,
            Some(sent_at + SignedDuration::from_hours(7 * 24))
        );
        assert!(
            invites.mailer.sent()[0]
                .body
                .contains("The link works once and expires in 7 days.")
        );
    }

    #[tokio::test]
    async fn a_second_send_for_the_same_invite_sends_nothing() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        invites.send(invite_id).await.unwrap();
        let token = invites.last_token();

        invites.send(invite_id).await.unwrap();

        assert_eq!(invites.mailer.sent().len(), 1);
        assert!(invites.accept(&token).await.is_ok());
    }

    #[tokio::test]
    async fn a_cancelled_or_accepted_invite_sends_nothing() {
        let invites = Invites::new().await;
        let cancelled = invites.queue().await;
        cancel_invite::call(
            cancel_invite::Dependencies {
                db_context: invites.db_context.clone(),
                acquire_lock: invites.lock.port(),
            },
            cancelled,
        )
        .await
        .unwrap();
        invites.send(cancelled).await.unwrap();
        let accepted = invites.queue().await;
        invites.send(accepted).await.unwrap();
        invites.resend(accepted).await;
        invites.accept(&invites.last_token()).await.unwrap();

        invites.send(accepted).await.unwrap();

        assert_eq!(invites.mailer.sent().len(), 1);
    }

    #[tokio::test]
    async fn a_refused_address_marks_the_invite_rejected() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        let logs = Logs::default();
        let writer = logs.clone();
        let _logging = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry().with(LevelFilter::INFO).with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(move || writer.clone()),
            ),
        );

        let result = invites.send_with(invite_id, refusing_mailer()).await;

        assert!(result.is_ok(), "{result:?}");
        let invite = invites.invite(invite_id).await.unwrap();
        assert_eq!(
            invite.email_status,
            InviteEmailStatus::Rejected {
                rejected_at: invites.clock.now()
            }
        );
        assert_eq!(invite.token_hash, None);
        let logs = logs.text();
        assert_eq!(logs.matches(" WARN ").count(), 1, "{logs}");
        assert!(logs.contains("the mail server refused the email"), "{logs}");
        assert!(!logs.contains(EMAIL), "{logs}");
    }

    #[tokio::test]
    async fn a_send_that_fails_leaves_the_invite_queued_without_a_token() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        let queued_at = invites.clock.now();
        invites.clock.advance(SignedDuration::from_hours(1));

        let result = invites.send_with(invite_id, failing_mailer()).await;

        assert!(result.is_err());
        let invite = invites.invite(invite_id).await.unwrap();
        assert_eq!(invite.email_status, InviteEmailStatus::Queued { queued_at });
        assert_eq!(invite.token_hash, None);
        assert_eq!(invite.expires_at, None);
    }

    #[tokio::test]
    async fn a_failed_send_after_a_resend_keeps_the_old_link_working() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        invites.send(invite_id).await.unwrap();
        let old = invites.last_token();
        invites.resend(invite_id).await;

        let result = invites.send_with(invite_id, failing_mailer()).await;

        assert!(result.is_err());
        assert!(invites.accept(&old).await.is_ok());
    }

    #[tokio::test]
    async fn after_a_resend_is_sent_only_the_new_link_works() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;
        invites.send(invite_id).await.unwrap();
        let old = invites.last_token();
        invites.resend(invite_id).await;
        invites.clock.advance(SignedDuration::from_hours(24));

        invites.send(invite_id).await.unwrap();

        let new = invites.last_token();
        let invite = invites.invite(invite_id).await.unwrap();
        assert_eq!(
            invite.expires_at,
            Some(invites.clock.now() + SignedDuration::from_hours(7 * 24))
        );
        let with_old = invites
            .accept(&old)
            .await
            .expect_err("the old link no longer works");
        assert!(
            with_old
                .find_source::<accept_invite::InviteNotFound>()
                .is_some(),
            "{with_old}"
        );
        assert!(invites.accept(&new).await.is_ok());
    }

    #[tokio::test]
    async fn sending_takes_the_same_lock_as_resend() {
        let invites = Invites::new().await;
        let invite_id = invites.queue().await;

        invites.resend(invite_id).await;
        invites.send(invite_id).await.unwrap();

        let key = format!("invites:email:v1:{invite_id}");
        assert_eq!(invites.lock.keys(), [key.clone(), key]);
    }
}
