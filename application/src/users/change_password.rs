mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::WrongCurrentPassword;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Password;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    token: &str,
    current_password: String,
    new_password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = db_context
        .users()
        .find(user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if !passwords::verify(current_password, user.map(|x| x.password_hash))
        .await
        .map_err(Error::caused_by)?
    {
        return Err(Error::caused_by(WrongCurrentPassword::new()));
    }
    let password_hash = passwords::hash(new_password)
        .await
        .map_err(Error::caused_by)?;
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
        .filter(
            Session::fields()
                .user_id()
                .eq(user_id)
                .and(Session::fields().token_hash().ne(tokens::hash(token))),
        )
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
    use crate::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    const PASSWORD: &str = "a long passphrase";
    const NEW_PASSWORD: &str = "a new passphrase";

    async fn create_user(db_context: &DbContext, name: &str) -> Uuid {
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            EmailAddress::new(format!("{name}@example.com")).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

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

    async fn change_password(
        db_context: &DbContext,
        user_id: Uuid,
        token: &str,
        current_password: &str,
    ) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            user_id,
            token,
            current_password.to_owned(),
            Password::new(NEW_PASSWORD.to_owned()).unwrap(),
        )
        .await
    }

    async fn token_hashes(db_context: &mut DbContext) -> Vec<Vec<u8>> {
        let mut hashes = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        hashes.sort();
        hashes
    }

    fn hashes<const N: usize>(tokens: [&str; N]) -> Vec<Vec<u8>> {
        let mut hashes = tokens.map(tokens::hash).to_vec();
        hashes.sort();
        hashes
    }

    #[tokio::test]
    async fn changing_the_password_ends_every_other_session_of_the_user() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        create_user(&db_context, "Ravi").await;
        let current = sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await.unwrap();
        for client_kind in [ClientKind::Web, ClientKind::AppleTv, ClientKind::GoogleTv] {
            sign_in(&db_context, "Noor", PASSWORD, client_kind).await.unwrap();
        }
        let ravi = sign_in(&db_context, "Ravi", PASSWORD, ClientKind::Web).await.unwrap();

        change_password(&db_context, noor, &current, PASSWORD)
            .await
            .unwrap();

        assert_eq!(token_hashes(&mut db_context).await, hashes([&current, &ravi]));
        assert_eq!(sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await, None);
        assert!(sign_in(&db_context, "Noor", NEW_PASSWORD, ClientKind::Web).await.is_some());
    }

    #[tokio::test]
    async fn a_wrong_current_password_returns_wrong_current_password_and_changes_nothing() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let current = sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await.unwrap();
        let tv = sign_in(&db_context, "Noor", PASSWORD, ClientKind::AppleTv).await.unwrap();

        let error = change_password(&db_context, noor, &current, "not the password")
            .await
            .expect_err("the current password is wrong");

        assert!(error.find_source::<WrongCurrentPassword>().is_some(), "{error}");
        assert_eq!(token_hashes(&mut db_context).await, hashes([&current, &tv]));
        assert!(sign_in(&db_context, "Noor", PASSWORD, ClientKind::Web).await.is_some());
    }
}
