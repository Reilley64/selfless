use crate::DbContext;
use crate::DbScope;
use crate::ports::Clock;
use domain::Session;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to list a user's sessions")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
) -> Result<Vec<Session>, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let mut sessions = db_context
        .sessions()
        .filter(Session::fields().user_id().eq(user_id))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    sessions.retain(|x| !x.expired(now));
    sessions.sort_by_key(|x| x.signed_in_at);
    Ok(sessions)
}
