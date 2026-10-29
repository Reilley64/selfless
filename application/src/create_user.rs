use crate::DbContext;
use crate::DbScope;
use crate::passwords;
use crate::unique_index;
use domain::Colour;
use domain::Symbol;
use domain::User;

const NAME_INDEX: &str = "index_users_by_name";

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to create a user")]
pub struct Error;

#[ohno::error]
#[display("a user with this name already exists")]
pub struct NameTaken;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    name: String,
    password: String,
    email: String,
) -> Result<User, Error> {
    let mut db_context = dependencies.into().db_context;
    let users = db_context
        .users()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let colours = users.iter().map(|x| x.colour).collect::<Vec<_>>();
    let symbols = users.iter().map(|x| x.symbol).collect::<Vec<_>>();
    let colour = least_used(&Colour::ALL, &colours);
    let symbol = least_used(&Symbol::ALL, &symbols);
    let password_hash = passwords::hash(password).await.map_err(Error::caused_by)?;
    db_context
        .users()
        .create()
        .name(name)
        .email(email)
        .password_hash(password_hash)
        .colour(colour)
        .symbol(symbol)
        .exec(db_context.executor_mut())
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
