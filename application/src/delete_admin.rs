use crate::DbContext;
use crate::DbScope;
use domain::AdminSession;
use uuid::Uuid;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
}

#[ohno::error]
#[display("failed to delete an admin")]
pub struct Error;

#[ohno::error]
#[display("the last admin cannot be deleted")]
pub struct LastAdmin;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<(), Error> {
    let mut db_context = dependencies.into().db_context;
    let deleted = toasty::sql::statement(
        "DELETE FROM admins WHERE id = $1 \
         AND (SELECT count(*) FROM (SELECT id FROM admins FOR UPDATE) AS locked) > 1",
    )
    .bind(id)
    .exec(db_context.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    if deleted == 0
        && db_context
            .admins()
            .find(id)
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .is_some()
    {
        return Err(Error::caused_by(LastAdmin::new()));
    }
    db_context
        .admin_sessions()
        .filter(AdminSession::fields().admin_id().eq(id))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)
}
