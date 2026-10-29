use crate::DbContext;
use crate::ports::Clock;
use crate::session_tokens;
use domain::AdminSession;
use jiff::SignedDuration;
use uuid::Uuid;

const IDLE_TIMEOUT: SignedDuration = SignedDuration::from_hours(12);
const LAST_USED_PRECISION: SignedDuration = SignedDuration::from_hours(1);

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to get the current admin")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
) -> Result<Option<Uuid>, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let Some(mut session) = AdminSession::filter_by_token_hash(session_tokens::hash(token))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| now.duration_since(x.last_used_at) < IDLE_TIMEOUT)
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
    Ok(Some(session.admin_id))
}
