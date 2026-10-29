mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::AdminSession;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
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
        .map_err(Error::caused_by)?;
    Ok(Output)
}
