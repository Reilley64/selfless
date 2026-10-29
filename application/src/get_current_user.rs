use crate::DbContext;
use crate::ports::Clock;
use crate::tokens;
use domain::ClientKind;
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
    let Some(mut session) = Session::filter_by_token_hash(tokens::hash(token))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| {
            x.client_kind != ClientKind::Web
                || now.duration_since(x.last_used_at) < Session::WEB_IDLE_TIMEOUT
        })
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
    User::filter_by_id(session.user_id)
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
