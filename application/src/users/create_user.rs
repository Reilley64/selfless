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
use domain::Symbol;
use domain::User;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    email: String,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let user = create(&mut db_context, name, password, email).await?;
    Ok(Output { id: user.id })
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
    User::upsert_by_name(name)
        .email(email)
        .password_hash(password_hash)
        .colour(least_used(&Colour::ALL, &colours))
        .symbol(least_used(&Symbol::ALL, &symbols))
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
