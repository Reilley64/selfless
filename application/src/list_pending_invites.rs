use crate::DbContext;
use domain::Invite;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list pending invites")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<Invite>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut invites = Invite::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    invites.sort_by_key(|x| std::cmp::Reverse(x.sent_at));
    Ok(invites)
}
