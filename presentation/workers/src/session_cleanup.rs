use application::sessions::delete_expired_sessions;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    delete_expired_sessions::call(state).await?;
    Ok(())
}

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut daily = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(daily.tick())
        .await
        .is_some()
    {
        if let Err(error) = delete_expired_sessions::call(&state).await {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to delete expired sessions"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::persistence::DbScope;
    use domain::ClientKind;
    use domain::Colour;
    use domain::EmailAddress;
    use domain::Symbol;
    use jiff::SignedDuration;
    use test_app::TestApp;
    use uuid::Uuid;

    #[tokio::test]
    async fn starting_deletes_expired_sessions() {
        let app = TestApp::new().await;
        let mut db_context = app.state.db_context.clone();
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
        for (device_name, client_kind) in [
            ("Old laptop", ClientKind::Web),
            ("Living Room", ClientKind::AppleTv),
        ] {
            db_context
                .sessions()
                .create()
                .user_id(user.id)
                .client_id(Uuid::new_v4())
                .client_kind(client_kind)
                .device_name(device_name)
                .token_hash(device_name.as_bytes().to_vec())
                .signed_in_at(app.clock.now())
                .last_used_at(app.clock.now())
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }
        app.clock.advance(SignedDuration::from_hours(30 * 24 + 1));

        start(&app.state).await.unwrap();

        let devices = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.device_name)
            .collect::<Vec<_>>();
        assert_eq!(devices, ["Living Room"]);
    }
}
