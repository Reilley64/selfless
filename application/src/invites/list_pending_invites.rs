mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut invites = db_context
        .invites()
        .all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    invites.sort_by_key(|x| std::cmp::Reverse(x.sent_at));
    Ok(Output {
        invites: invites
            .into_iter()
            .map(|x| output::Invite {
                id: x.id,
                email: x.email,
                sent_at: x.sent_at,
                expires_at: x.expires_at,
            })
            .collect(),
    })
}
