mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let Some(mut session) = db_context
        .sessions()
        .filter(Session::fields().token_hash().eq(tokens::hash(token)))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| !x.expired(now))
    else {
        return Ok(Output::NotSignedIn);
    };
    if session.last_used_is_stale(now) {
        session
            .update()
            .last_used_at(now)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    let user = db_context
        .users()
        .find(session.user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(user.map_or(Output::NotSignedIn, |user| Output::SignedIn {
        id: user.id,
        colour: user.colour(),
        name: user.name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::ClientKind;
    use domain::EmailAddress;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use test_fixtures::FakeClock;
    use uuid::Uuid;

    async fn create_user(db_context: &mut DbContext, name: &str) -> Uuid {
        db_context
            .users()
            .create()
            .name(name)
            .email(EmailAddress::new(format!("{name}@example.com")).unwrap())
            .password_hash("hash")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn add_web_session(
        db_context: &mut DbContext,
        user_id: Uuid,
        token: &str,
        last_used_at: Timestamp,
    ) {
        db_context
            .sessions()
            .create()
            .user_id(user_id)
            .client_id(Uuid::new_v4())
            .client_kind(ClientKind::Web)
            .device_name("Firefox on macOS")
            .token_hash(tokens::hash(token))
            .signed_in_at(last_used_at)
            .last_used_at(last_used_at)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn last_used_at(db_context: &mut DbContext) -> Timestamp {
        db_context
            .sessions()
            .all()
            .first()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .last_used_at
    }

    async fn get_current_user(db_context: &DbContext, clock: &FakeClock, token: &str) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            token,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn a_web_session_past_30_days_without_use_is_not_signed_in() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let noor = create_user(&mut db_context, "Noor").await;
        let days_ago = |x: i64| clock.now() - SignedDuration::from_hours(x * 24);
        add_web_session(&mut db_context, noor, "idle 30 days", days_ago(30)).await;
        add_web_session(&mut db_context, noor, "idle 29 days", days_ago(29)).await;

        let past_the_limit = get_current_user(&db_context, &clock, "idle 30 days").await;
        let inside_the_limit = get_current_user(&db_context, &clock, "idle 29 days").await;

        assert!(
            matches!(past_the_limit, Output::NotSignedIn),
            "{past_the_limit:?}"
        );
        assert!(
            matches!(inside_the_limit, Output::SignedIn { id, .. } if id == noor),
            "{inside_the_limit:?}"
        );
    }

    #[tokio::test]
    async fn last_used_is_written_at_most_once_an_hour() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let noor = create_user(&mut db_context, "Noor").await;
        let signed_in_at = clock.now();
        add_web_session(&mut db_context, noor, "token", signed_in_at).await;

        clock.advance(SignedDuration::from_mins(59));
        get_current_user(&db_context, &clock, "token").await;
        let after_59_minutes = last_used_at(&mut db_context).await;
        clock.advance(SignedDuration::from_mins(1));
        get_current_user(&db_context, &clock, "token").await;
        let after_an_hour = last_used_at(&mut db_context).await;

        assert_eq!(after_59_minutes, signed_in_at);
        assert_eq!(after_an_hour, signed_in_at + SignedDuration::from_hours(1));
    }
}
