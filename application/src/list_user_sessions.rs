use crate::DbContext;
use domain::Session;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to list a user's sessions")]
pub struct Error;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
) -> Result<Vec<Session>, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut sessions = Session::filter_by_user_id(user_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    sessions.sort_by_key(|x| x.signed_in_at);
    Ok(sessions)
}
