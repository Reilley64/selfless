mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Session;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, user_id: Uuid) -> Result<Output, Error> {
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
    Ok(Output {
        sessions: sessions
            .into_iter()
            .map(|x| output::Session {
                id: x.id,
                client_kind: x.client_kind,
                device_name: x.device_name,
                signed_in_at: x.signed_in_at,
                last_used_at: x.last_used_at,
            })
            .collect(),
    })
}
