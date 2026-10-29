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
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let deleted = toasty::sql::statement(
        "DELETE FROM admins WHERE id = $1 \
         AND (SELECT count(*) FROM (SELECT id FROM admins FOR UPDATE) AS locked) > 1",
    )
    .bind(id)
    .exec(transaction.executor_mut())
    .await
    .map_err(Error::caused_by)?;
    if deleted == 0
        && transaction
            .admins()
            .find(id)
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?
            .is_some()
    {
        return Err(Error::caused_by(LastAdmin::new()));
    }
    transaction
        .admin_sessions()
        .filter(AdminSession::fields().admin_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)
}
