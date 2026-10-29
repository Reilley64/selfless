use crate::DbContext;
use crate::session_tokens;
use domain::AdminSession;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to sign out an admin")]
pub struct Error;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    AdminSession::filter_by_token_hash(session_tokens::hash(token))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
