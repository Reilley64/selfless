mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Admin;
use domain::Password;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: Password,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let admin = Admin::upsert_by_name(name)
        .password_hash(password_hash)
        .created_at(clock.call())
        .or_ignore()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(NameTaken::new()))?;
    Ok(Output { id: admin.id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::sessions::admin_sign_in;
    use crate::sessions::sign_in;
    use crate::test::mock_db_context;
    use crate::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use test_fixtures::FakeClock;
    use uuid::Uuid;

    const ADMIN_PASSWORD: &str = "an admin passphrase";
    const USER_PASSWORD: &str = "a user passphrase";

    async fn create_admin(db_context: &DbContext, name: &str) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            Password::new(ADMIN_PASSWORD.to_owned()).unwrap(),
        )
        .await
    }

    async fn create_user(
        db_context: &DbContext,
        name: &str,
    ) -> Result<create_user::Output, create_user::Error> {
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            Password::new(USER_PASSWORD.to_owned()).unwrap(),
            EmailAddress::new(format!("{name}@example.com")).unwrap(),
        )
        .await
    }

    async fn admin_signs_in(db_context: &DbContext, name: &str) -> bool {
        let output = admin_sign_in::call(
            admin_sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            ADMIN_PASSWORD.to_owned(),
        )
        .await
        .unwrap();
        matches!(output, admin_sign_in::Output::SignedIn { .. })
    }

    async fn user_signs_in(db_context: &DbContext, name: &str) -> bool {
        let output = sign_in::call(
            sign_in::Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            USER_PASSWORD.to_owned(),
            ClientKind::Web,
            "Firefox on macOS".to_owned(),
            Uuid::new_v4(),
        )
        .await
        .unwrap();
        matches!(output, sign_in::Output::SignedIn { .. })
    }

    #[tokio::test]
    async fn a_new_admin_can_sign_in() {
        let db_context = mock_db_context().await;

        create_admin(&db_context, "Ravi").await.unwrap();

        assert!(admin_signs_in(&db_context, "Ravi").await);
    }

    #[tokio::test]
    async fn admin_names_and_user_names_are_separate_sets() {
        let db_context = mock_db_context().await;
        create_user(&db_context, "Noor").await.unwrap();
        create_admin(&db_context, "Maya").await.unwrap();

        create_admin(&db_context, "Noor").await.unwrap();
        create_user(&db_context, "Maya").await.unwrap();

        assert!(admin_signs_in(&db_context, "Noor").await);
        assert!(user_signs_in(&db_context, "Maya").await);
    }
}
