mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Session;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, user_id: Uuid) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let mut sessions = db_context
        .sessions()
        .filter(Session::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    sessions.retain(|x| !x.expired(now));
    sessions.sort_by_key(|x| x.signed_in_at);
    Ok(Output {
        sessions: sessions
            .into_iter()
            .map(|x| output::Session {
                id: x.id,
                client_kind: x.client_kind,
                device_name: x.device_name,
                signed_in_at: x.signed_in_at,
                last_used_at: x.last_used_at,
            })
            .collect(),
    })
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

    async fn add_session(
        db_context: &mut DbContext,
        user_id: Uuid,
        client_kind: ClientKind,
        device_name: &str,
        signed_in_at: Timestamp,
        last_used_at: Timestamp,
    ) {
        db_context
            .sessions()
            .create()
            .user_id(user_id)
            .client_id(Uuid::new_v4())
            .client_kind(client_kind)
            .device_name(device_name)
            .token_hash(device_name.as_bytes().to_vec())
            .signed_in_at(signed_in_at)
            .last_used_at(last_used_at)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    async fn list(
        db_context: &DbContext,
        clock: &FakeClock,
        user_id: Uuid,
    ) -> Vec<(ClientKind, String, Timestamp, Timestamp)> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            user_id,
        )
        .await
        .unwrap()
        .sessions
        .into_iter()
        .map(|x| (x.client_kind, x.device_name, x.signed_in_at, x.last_used_at))
        .collect()
    }

    #[tokio::test]
    async fn listing_a_users_sessions_shows_each_client_kind_device_name_and_time() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let noor = create_user(&mut db_context, "Noor").await;
        let ravi = create_user(&mut db_context, "Ravi").await;
        let hours_ago = |x| clock.now() - SignedDuration::from_hours(x);
        add_session(
            &mut db_context,
            noor,
            ClientKind::AppleTv,
            "Living Room",
            hours_ago(2),
            hours_ago(2),
        )
        .await;
        add_session(
            &mut db_context,
            noor,
            ClientKind::Web,
            "Firefox on macOS",
            hours_ago(4),
            hours_ago(1),
        )
        .await;
        add_session(
            &mut db_context,
            ravi,
            ClientKind::GoogleTv,
            "Bedroom",
            hours_ago(3),
            hours_ago(3),
        )
        .await;

        let sessions = list(&db_context, &clock, noor).await;

        assert_eq!(
            sessions,
            [
                (
                    ClientKind::Web,
                    "Firefox on macOS".to_owned(),
                    hours_ago(4),
                    hours_ago(1),
                ),
                (
                    ClientKind::AppleTv,
                    "Living Room".to_owned(),
                    hours_ago(2),
                    hours_ago(2),
                ),
            ]
        );
    }

    #[tokio::test]
    async fn the_list_leaves_out_web_sessions_past_30_days_without_use() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        let noor = create_user(&mut db_context, "Noor").await;
        let days_ago = |x: i64| clock.now() - SignedDuration::from_hours(x * 24);
        for (client_kind, device_name, idle_days) in [
            (ClientKind::Web, "Old laptop", 30),
            (ClientKind::Web, "Firefox on macOS", 29),
            (ClientKind::GoogleTv, "Bedroom", 400),
        ] {
            add_session(
                &mut db_context,
                noor,
                client_kind,
                device_name,
                days_ago(400),
                days_ago(idle_days),
            )
            .await;
        }

        let sessions = list(&db_context, &clock, noor).await;

        let mut device_names = sessions.into_iter().map(|x| x.1).collect::<Vec<_>>();
        device_names.sort();
        assert_eq!(device_names, ["Bedroom", "Firefox on macOS"]);
    }
}
