mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::attributes::selfless_auth_client_kind;
use crate::ports::persistence::DbScope;
use crate::sessions::sign_in_attempts;
use crate::tokens;
use domain::ClientKind;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    client_kind: ClientKind,
    device_name: String,
    client_id: Uuid,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let user = db_context
        .users()
        .filter(User::fields().name().eq(name))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let matches = sign_in_attempts::check_and_count(
        password,
        user.as_ref().map(|x| x.password_hash.clone()),
        match client_kind {
            ClientKind::Web => selfless_auth_client_kind::WEB,
            ClientKind::AppleTv => selfless_auth_client_kind::APPLE_TV,
            ClientKind::GoogleTv => selfless_auth_client_kind::GOOGLE_TV,
        },
    )
    .await
    .map_err(Error::caused_by)?;
    let Some(user) = user.filter(|_| matches) else {
        tracing::warn!("user sign-in failed");
        return Ok(Output::WrongNameOrPassword);
    };
    let token = tokens::generate();
    let now = clock.call();
    Session::upsert_by_user_id_and_client_id(user.id, client_id)
        .client_kind(client_kind)
        .device_name(device_name)
        .token_hash(tokens::hash(&token))
        .signed_in_at(now)
        .last_used_at(now)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output::SignedIn {
        token,
        user_id: user.id,
        name: user.name,
        colour: user.colour,
        symbol: user.symbol,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use crate::users::create_user;
    use domain::EmailAddress;
    use domain::Password;
    use jiff::SignedDuration;
    use jiff::Timestamp;
    use test_fixtures::FakeClock;

    const PASSWORD: &str = "a long passphrase";

    async fn sign_in(
        db_context: &DbContext,
        clock: &FakeClock,
        password: &str,
        device_name: &str,
        client_id: Uuid,
    ) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(clock.reader()),
            },
            "Noor".to_owned(),
            password.to_owned(),
            ClientKind::AppleTv,
            device_name.to_owned(),
            client_id,
        )
        .await
        .unwrap()
    }

    async fn sessions(db_context: &mut DbContext) -> Vec<(Uuid, String, Vec<u8>, Timestamp)> {
        db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| (x.id, x.device_name, x.token_hash, x.signed_in_at))
            .collect()
    }

    #[tokio::test]
    async fn a_wrong_password_changes_no_session() {
        let mut db_context = mock_db_context().await;
        let clock = FakeClock::new();
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
            },
            "Noor".to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            EmailAddress::new("noor@example.com".to_owned()).unwrap(),
        )
        .await
        .unwrap();
        let client_id = Uuid::new_v4();
        sign_in(&db_context, &clock, PASSWORD, "Living Room", client_id).await;
        let before = sessions(&mut db_context).await;
        clock.advance(SignedDuration::from_hours(2));

        let output = sign_in(&db_context, &clock, "wrong", "Bedroom", client_id).await;

        assert!(matches!(output, Output::WrongNameOrPassword));
        assert_eq!(sessions(&mut db_context).await, before);
    }
}
