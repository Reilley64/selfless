mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Password;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .users()
        .filter(User::fields().id().eq(user_id))
        .update()
        .password_hash(password_hash)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .sessions()
        .filter(Session::fields().user_id().eq(user_id))
        .delete()
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
    use crate::sessions::sign_in;
    use crate::test::mock_db_context;
    use crate::tokens;
    use crate::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use test_fixtures::FakeClock;

    const PASSWORD: &str = "a long passphrase";

    async fn create_user(db_context: &DbContext, name: &str) -> Uuid {
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            EmailAddress::new(format!("{name}@example.com")).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

    const NEW_PASSWORD: &str = "a new passphrase";

    async fn sign_in(
        db_context: &DbContext,
        name: &str,
        password: &str,
        client_kind: ClientKind,
    ) -> Option<String> {
        let output = sign_in::call(
            sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            password.to_owned(),
            client_kind,
            "Living Room".to_owned(),
            Uuid::new_v4(),
        )
        .await
        .unwrap();
        match output {
            sign_in::Output::SignedIn { token, .. } => Some(token),
            sign_in::Output::WrongNameOrPassword => None,
        }
    }

    #[tokio::test]
    async fn resetting_a_users_password_ends_all_their_sessions() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        create_user(&db_context, "Ravi").await;
        for client_kind in [ClientKind::Web, ClientKind::GoogleTv] {
            sign_in(&db_context, "Noor", PASSWORD, client_kind)
                .await
                .unwrap();
        }
        let ravi = sign_in(&db_context, "Ravi", PASSWORD, ClientKind::Web)
            .await
            .unwrap();

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            noor,
            Password::new(NEW_PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap();

        let sessions = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        assert_eq!(sessions, [tokens::hash(&ravi)]);
        assert_eq!(
            sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await,
            None
        );
        assert!(
            sign_in(&db_context, "Noor", NEW_PASSWORD, ClientKind::Web)
                .await
                .is_some()
        );
    }
}
