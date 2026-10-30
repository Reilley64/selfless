mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use domain::Colour;
use domain::EmailAddress;
use domain::Password;
use domain::Symbol;
use domain::User;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: Password,
    email: EmailAddress,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = create(&mut db_context, name, password, email, None).await?;
    Ok(Output { id: user.id })
}

pub(crate) async fn create(
    scope: &mut impl DbScope,
    name: String,
    password: Password,
    email: EmailAddress,
    idempotency_key: Option<Uuid>,
) -> Result<User, Error> {
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    let users = scope
        .users()
        .all()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let colours = users.iter().map(|x| x.colour).collect::<Vec<_>>();
    let symbols = users.iter().map(|x| x.symbol).collect::<Vec<_>>();
    User::upsert_by_name(name)
        .email(email)
        .password_hash(password_hash)
        .colour(least_used(&Colour::ALL, &colours))
        .symbol(least_used(&Symbol::ALL, &symbols))
        .idempotency_key(idempotency_key)
        .or_ignore()
        .exec(scope.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .ok_or_else(|| Error::caused_by(NameTaken::new()))
}

fn least_used<T: Copy + PartialEq>(choices: &[T], used: &[T]) -> T {
    *choices
        .iter()
        .min_by_key(|choice| used.iter().filter(|x| x == choice).count())
        .expect("there is at least one choice")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use ohno::ErrorExt;

    const NAMES: [&str; 5] = ["Noor", "Maya", "Ravi", "Ines", "Tomas"];

    async fn create_user(db_context: &DbContext, name: &str, email: &str) -> Result<Output, Error> {
        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new("a long passphrase".to_owned()).unwrap(),
            EmailAddress::new(email.to_owned()).unwrap(),
        )
        .await
    }

    #[tokio::test]
    async fn new_users_get_the_colour_and_symbol_used_by_the_fewest_users() {
        let mut db_context = mock_db_context().await;
        for name in NAMES {
            let email = format!("{}@example.com", name.to_lowercase());
            create_user(&db_context, name, &email).await.unwrap();
        }

        let users = db_context
            .users()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        let mut looks = users
            .iter()
            .map(|x| (x.name.as_str(), x.colour, x.symbol))
            .collect::<Vec<_>>();
        looks.sort_by_key(|x| NAMES.iter().position(|y| *y == x.0));
        assert_eq!(
            looks,
            [
                ("Noor", Colour::Amber, Symbol::Ring),
                ("Maya", Colour::Blue, Symbol::Triangle),
                ("Ravi", Colour::Pink, Symbol::Star),
                ("Ines", Colour::Emerald, Symbol::Waves),
                ("Tomas", Colour::Amber, Symbol::Ring),
            ]
        );
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
