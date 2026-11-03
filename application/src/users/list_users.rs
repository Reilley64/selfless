mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let mut users = db_context
        .users()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    users.sort_by_cached_key(|x| x.name.to_lowercase());
    let mut sessions = db_context
        .sessions()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    sessions.retain(|x| !x.expired(now));
    Ok(Output {
        users: users
            .into_iter()
            .map(|user| {
                let signed_in = sessions.iter().filter(|x| x.user_id == user.id);
                output::User {
                    colour: user.colour(),
                    signed_in_devices: signed_in.clone().count(),
                    last_used_at: signed_in.map(|x| x.last_used_at).max(),
                    id: user.id,
                    name: user.name,
                    created_at: user.created_at,
                }
            })
            .collect(),
    })
}
