mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::AdminSession;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let Some(mut session) = db_context
        .admin_sessions()
        .filter(AdminSession::fields().token_hash().eq(tokens::hash(token)))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| now.duration_since(x.last_used_at) < AdminSession::IDLE_TIMEOUT)
    else {
        return Ok(Output::NotSignedIn);
    };
    if now.duration_since(session.last_used_at) >= AdminSession::LAST_USED_PRECISION {
        session
            .update()
            .last_used_at(now)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
    }
    Ok(Output::SignedIn {
        admin_id: session.admin_id,
    })
}
