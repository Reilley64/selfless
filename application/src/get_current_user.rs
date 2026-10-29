use crate::DbContext;
use crate::DbScope;
use crate::ports::Clock;
use crate::tokens;
use domain::LAST_USED_PRECISION;
use domain::Session;
use domain::User;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to get the current user")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
) -> Result<Option<User>, Error> {
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
        return Ok(None);
    };
    if now.duration_since(session.last_used_at) >= LAST_USED_PRECISION {
        session
            .update()
            .last_used_at(now)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    db_context
        .users()
        .find(session.user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
