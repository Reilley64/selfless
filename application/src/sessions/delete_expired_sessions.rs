mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::AdminSession;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    db_context
        .sessions()
        .filter(
            Session::fields().client_kind().is_web().and(
                Session::fields()
                    .last_used_at()
                    .le(Session::web_idle_cutoff(now)),
            ),
        )
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    db_context
        .admin_sessions()
        .filter(
            AdminSession::fields()
                .last_used_at()
                .le(AdminSession::idle_cutoff(now)),
        )
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
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::ClientKind;
    use domain::Colour;
    use domain::EmailAddress;
    use domain::Symbol;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use uuid::Uuid;

    const NOW: &str = "2026-06-01T12:00:00Z";

    fn idle_for(hours: i64) -> Timestamp {
        NOW.parse::<Timestamp>().unwrap() - SignedDuration::from_hours(hours)
    }

    async fn seed(db_context: &mut DbContext) {
        let user = db_context
            .users()
            .create()
            .name("Noor")
            .email(EmailAddress::new("noor@example.com".to_owned()).unwrap())
            .password_hash("hash")
            .colour(Colour::Amber)
            .symbol(Symbol::Ring)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (device_name, client_kind, idle_hours) in [
            ("Old laptop", ClientKind::Web, 30 * 24 + 1),
            ("Firefox on macOS", ClientKind::Web, 24),
            ("Living Room", ClientKind::AppleTv, 400 * 24),
            ("Bedroom", ClientKind::GoogleTv, 400 * 24),
        ] {
            db_context
                .sessions()
                .create()
                .user_id(user.id)
                .client_id(Uuid::new_v4())
                .client_kind(client_kind)
                .device_name(device_name)
                .token_hash(device_name.as_bytes().to_vec())
                .signed_in_at(idle_for(idle_hours))
                .last_used_at(idle_for(idle_hours))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        let admin = db_context
            .admins()
            .create()
            .name("Maya")
            .password_hash("hash")
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        for (token, idle_hours) in [("old", 12 + 1), ("recent", 1)] {
            db_context
                .admin_sessions()
                .create()
                .admin_id(admin.id)
                .token_hash(token.as_bytes().to_vec())
                .signed_in_at(idle_for(idle_hours))
                .last_used_at(idle_for(idle_hours))
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn deletes_only_web_sessions_and_admin_sessions_past_their_idle_limit() {
        let mut db_context = mock_db_context().await;
        seed(&mut db_context).await;

        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(|| NOW.parse().unwrap()),
        })
        .await
        .unwrap();

        let mut devices = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.device_name)
            .collect::<Vec<_>>();
        devices.sort();
        let admin_sessions = db_context
            .admin_sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        assert_eq!(devices, ["Bedroom", "Firefox on macOS", "Living Room"]);
        assert_eq!(admin_sessions, [b"recent".to_vec()]);
    }
}
