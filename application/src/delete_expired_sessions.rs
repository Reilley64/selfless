use crate::DbContext;
use crate::DbScope;
use crate::ports::Clock;
use domain::AdminSession;
use domain::Session;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to delete expired sessions")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<(), Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    db_context
        .sessions()
        .filter(
            Session::fields().client_kind().is_web().and(
                Session::fields()
                    .last_used_at()
                    .le(now - Session::WEB_IDLE_TIMEOUT),
            ),
        )
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    db_context
        .admin_sessions()
        .filter(
            AdminSession::fields()
                .last_used_at()
                .le(now - AdminSession::IDLE_TIMEOUT),
        )
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
