mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::EmailAddress;
use domain::Password;
use domain::User;
use jiff::Timestamp;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: Password,
    email: EmailAddress,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let user = create(&mut db_context, name, password, email, None, clock.call()).await?;
    Ok(Output { id: user.id })
}

pub(crate) async fn create(
    scope: &mut impl DbScope,
    name: String,
    password: Password,
    email: EmailAddress,
    idempotency_key: Option<Uuid>,
    created_at: Timestamp,
) -> Result<User, Error> {
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    User::upsert_by_name(name)
        .email(email)
        .password_hash(password_hash)
        .idempotency_key(idempotency_key)
        .created_at(created_at)
        .or_ignore()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(NameTaken::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    async fn create_user(db_context: &DbContext, name: &str, email: &str) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            name.to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            EmailAddress::new(email.to_owned()).unwrap(),
        )
        .await
    }

    #[tokio::test]
    async fn a_taken_name_returns_name_taken() {
        let db_context = mock_db_context().await;
        create_user(&db_context, "Noor", "noor@example.com")
            .await
            .unwrap();

        let error = create_user(&db_context, "Noor", "someone@example.com")
            .await
            .expect_err("the name is taken");

        assert!(error.find_source::<NameTaken>().is_some(), "{error}");
    }
}
