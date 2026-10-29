mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use error::NameTaken;
pub use output::Output;

use crate::passwords;
use crate::ports::persistence::DbScope;
use crate::ports::persistence::unique_index;
use domain::Colour;
use domain::Symbol;
use domain::User;

const NAME_INDEX: &str = "index_users_by_name";

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    email: String,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = create(&mut db_context, name, password, email).await?;
    Ok(Output { user })
}

pub(crate) async fn create(
    scope: &mut impl DbScope,
    name: String,
    password: String,
    email: String,
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
    scope
        .users()
        .create()
        .name(name)
        .email(email)
        .password_hash(password_hash)
        .colour(least_used(&Colour::ALL, &colours))
        .symbol(least_used(&Symbol::ALL, &symbols))
        .exec(scope.executor_mut())
        .await
        .map_err(|error| match unique_index::violated(&error) {
            Some(NAME_INDEX) => Error::caused_by(NameTaken::new()),
            _ => Error::caused_by(error),
        })
}

fn least_used<T: Copy + PartialEq>(choices: &[T], used: &[T]) -> T {
    *choices
        .iter()
        .min_by_key(|choice| used.iter().filter(|x| x == choice).count())
        .expect("there is at least one choice")
}
