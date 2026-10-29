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
                    .le(Session::web_idle_cutoff(now)),
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
                .le(AdminSession::idle_cutoff(now)),
        )
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}
