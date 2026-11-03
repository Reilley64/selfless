mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Admin;
use domain::AdminSession;
use domain::Password;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    id: Uuid,
    password: Password,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .admins()
        .filter(Admin::fields().id().eq(id))
        .update()
        .password_hash(password_hash)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .admin_sessions()
        .filter(AdminSession::fields().admin_id().eq(id))
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
    use crate::admins::create_admin;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::sessions::admin_sign_in;
    use crate::test::mock_db_context;
    use crate::tokens;
    use test_fixtures::FakeClock;

    const PASSWORD: &str = "a long passphrase";
    const NEW_PASSWORD: &str = "a new passphrase";

    async fn create_admin(db_context: &DbContext, name: &str) -> Uuid {
        create_admin::call(
            create_admin::Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

    async fn admin_sign_in(db_context: &DbContext, name: &str, password: &str) -> Option<String> {
        let output = admin_sign_in::call(
            admin_sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            password.to_owned(),
        )
        .await
        .unwrap();
        match output {
            admin_sign_in::Output::SignedIn { token } => Some(token),
            admin_sign_in::Output::WrongNameOrPassword => None,
        }
    }

    #[tokio::test]
    async fn changing_an_admins_password_ends_their_sessions() {
        let mut db_context = mock_db_context().await;
        let maya = create_admin(&db_context, "Maya").await;
        create_admin(&db_context, "Ravi").await;
        admin_sign_in(&db_context, "Maya", PASSWORD).await.unwrap();
        admin_sign_in(&db_context, "Maya", PASSWORD).await.unwrap();
        let ravi = admin_sign_in(&db_context, "Ravi", PASSWORD).await.unwrap();

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            maya,
            Password::new(NEW_PASSWORD.to_owned()).unwrap(),
        )
        .await
        .unwrap();

        let admin_sessions = db_context
            .admin_sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        assert_eq!(admin_sessions, [tokens::hash(&ravi)]);
        assert_eq!(admin_sign_in(&db_context, "Maya", PASSWORD).await, None);
        assert!(
            admin_sign_in(&db_context, "Maya", NEW_PASSWORD)
                .await
                .is_some()
        );
    }
}
