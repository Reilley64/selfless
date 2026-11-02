mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::AddressTaken;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::trace_context;
use domain::EmailAddress;
use domain::Invite;
use domain::InviteEmailStatus;
use domain::OutboxData;
use domain::User;
use jiff::Timestamp;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    admin_id: Uuid,
    email: EmailAddress,
    origin: String,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let user = transaction
        .users()
        .filter(User::fields().email().eq(email.clone()))
        .first()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if user.is_some() {
        return Err(Error::caused_by(AddressTaken::new()));
    }
    let invite = Invite::upsert_by_email(email)
        .token_hash(None::<Vec<u8>>)
        .admin_id(admin_id)
        .email_status(InviteEmailStatus::Queued {
            queued_at: clock.call(),
        })
        .expires_at(None::<Timestamp>)
        .or_ignore()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(AddressTaken::new()))?;
    transaction
        .outboxes()
        .create()
        .data(OutboxData::SendInviteEmail {
            invite_id: invite.id,
            origin,
        })
        .traceparent(trace_context::current_traceparent())
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
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::Outbox;
    use ohno::ErrorExt;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_sdk::trace::InMemorySpanExporter;
    use opentelemetry_sdk::trace::SdkTracerProvider;
    use test_fixtures::FakeClock;
    use tracing::Instrument;
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const ORIGIN: &str = "https://selfless.example";

    async fn invite(
        db_context: &DbContext,
        clock: &FakeClock,
        email: &str,
    ) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            Uuid::nil(),
            EmailAddress::new(email.to_owned()).unwrap(),
            ORIGIN.to_owned(),
        )
        .await
    }

    async fn invites(db_context: &mut DbContext) -> Vec<Invite> {
        db_context
            .invites()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    async fn outboxes(db_context: &mut DbContext) -> Vec<Outbox> {
        db_context
            .outboxes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn inviting_queues_the_invite_with_an_outbox_row_and_no_token() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let admin_id = Uuid::new_v4();

        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            admin_id,
            EmailAddress::new("noor@household.example".to_owned()).unwrap(),
            ORIGIN.to_owned(),
        )
        .await
        .unwrap();

        let [invite] = invites(&mut db_context).await.try_into().unwrap();
        assert_eq!(invite.admin_id, admin_id);
        assert_eq!(
            invite.email_status,
            InviteEmailStatus::Queued {
                queued_at: clock.now()
            }
        );
        assert_eq!(invite.token_hash, None);
        assert_eq!(invite.expires_at, None);
        let [outbox] = outboxes(&mut db_context).await.try_into().unwrap();
        assert_eq!(
            outbox.data,
            OutboxData::SendInviteEmail {
                invite_id: invite.id,
                origin: ORIGIN.to_owned(),
            }
        );
        assert_eq!(outbox.delivered_at, None);
    }

    #[tokio::test]
    async fn an_address_that_already_has_a_user_or_a_pending_invite_creates_nothing() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        db_context
            .users()
            .create()
            .name("Noor")
            .email(EmailAddress::new("noor@household.example".to_owned()).unwrap())
            .password_hash("")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        invite(&db_context, &clock, "ravi@household.example")
            .await
            .unwrap();

        let user = invite(&db_context, &clock, "noor@household.example").await;
        let pending = invite(&db_context, &clock, "ravi@household.example").await;

        for result in [user, pending] {
            let error = result.expect_err("the address is taken");
            assert!(error.find_source::<AddressTaken>().is_some(), "{error}");
        }
        assert_eq!(invites(&mut db_context).await.len(), 1);
        assert_eq!(outboxes(&mut db_context).await.len(), 1);
    }

    #[tokio::test]
    async fn the_outbox_row_stores_the_current_trace_context() {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let _tracing = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::INFO)
                .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("selfless"))),
        );
        let mut db_context = mock_db_context().await;
        let request = tracing::info_span!("request");

        invite(&db_context, &FakeClock::new(), "noor@household.example")
            .instrument(request.clone())
            .await
            .unwrap();

        drop(request);
        let spans = exporter.get_finished_spans().unwrap();
        let request = spans.iter().find(|x| x.name == "request").unwrap();
        let [outbox] = outboxes(&mut db_context).await.try_into().unwrap();
        assert_eq!(
            outbox.traceparent,
            Some(format!(
                "00-{}-{}-01",
                request.span_context.trace_id(),
                request.span_context.span_id()
            ))
        );
    }
}
