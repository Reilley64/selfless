mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::MissingFirstAdmin;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Password;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    first_admin: Option<(String, Password)>,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    if db_context
        .admins()
        .all()
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .is_some()
    {
        return Ok(Output);
    }
    let Some((name, password)) = first_admin else {
        return Err(Error::caused_by(MissingFirstAdmin::new()));
    };
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    db_context
        .admins()
        .create()
        .name(name)
        .password_hash(password_hash)
        .created_at(clock.call())
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
    use ohno::ErrorExt;
    use test_fixtures::FakeClock;

    fn maya() -> Option<(String, Password)> {
        Some((
            "Maya".to_owned(),
            Password::new("correct horse".to_owned()).unwrap(),
        ))
    }

    async fn admins(db_context: &mut DbContext) -> Vec<(String, bool)> {
        let mut admins = Vec::new();
        for admin in db_context
            .admins()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
        {
            let verified = passwords::verify("correct horse".to_owned(), Some(admin.password_hash))
                .await
                .unwrap();
            admins.push((admin.name, verified));
        }
        admins
    }

    #[tokio::test]
    async fn without_an_admin_the_given_name_and_password_become_the_first_admin() {
        let mut db_context = mock_db_context().await;

        call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            maya(),
        )
        .await
        .unwrap();

        assert_eq!(admins(&mut db_context).await, [("Maya".to_owned(), true)]);
    }

    #[tokio::test]
    async fn without_an_admin_or_a_first_admin_it_fails_with_missing_first_admin() {
        let mut db_context = mock_db_context().await;

        let error = call(
            Dependencies {
                db_context: db_context.clone(),
                clock: Clock::new(FakeClock::new().reader()),
            },
            None,
        )
        .await
        .expect_err("there is no admin to create");

        assert!(
            error.find_source::<MissingFirstAdmin>().is_some(),
            "{error}"
        );
        assert!(admins(&mut db_context).await.is_empty());
    }

    #[tokio::test]
    async fn once_an_admin_exists_the_first_admin_is_ignored() {
        let mut db_context = mock_db_context().await;
        db_context
            .admins()
            .create()
            .name("Ravi")
            .password_hash("hash")
            .created_at(jiff::Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        for first_admin in [maya(), None] {
            call(
                Dependencies {
                    db_context: db_context.clone(),
                    clock: Clock::new(FakeClock::new().reader()),
                },
                first_admin,
            )
            .await
            .unwrap();
        }

        let names = db_context
            .admins()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["Ravi"]);
    }
}
