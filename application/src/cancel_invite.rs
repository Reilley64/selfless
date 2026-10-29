use crate::DbContext;
use crate::DbScope;
use domain::Invite;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to cancel an invite")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .invites()
        .filter(Invite::fields().id().eq(id))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
