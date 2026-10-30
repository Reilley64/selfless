mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let Some(mut session) = db_context
        .sessions()
        .filter(Session::fields().token_hash().eq(tokens::hash(token)))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| !x.expired(now))
    else {
        return Ok(Output::NotSignedIn);
    };
    if session.last_used_is_stale(now) {
        session
            .update()
            .last_used_at(now)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    let user = db_context
        .users()
        .find(session.user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(user.map_or(Output::NotSignedIn, |user| Output::SignedIn {
        id: user.id,
        name: user.name,
        colour: user.colour,
        symbol: user.symbol,
    }))
}
