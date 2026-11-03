mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::AcquireLock;
use crate::ports::persistence::DbScope;
use domain::Admin;
use domain::AdminSession;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        acquire_lock,
    } = dependencies.into();
    let _guard = acquire_lock
        .call(AcquireLock::key("admins", "delete", 1, []))
        .await;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    let Some(admin) = transaction
        .admins()
        .find(id)
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output);
    };
    let admins = transaction
        .admins()
        .all()
        .count()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Admin::can_delete(admins).map_err(Error::caused_by)?;
    admin
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .admin_sessions()
        .filter(AdminSession::fields().admin_id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::RecordingLock;
    use crate::test::mock_db_context;
    use domain::LastAdmin;
    use jiff::Timestamp;
    use ohno::ErrorExt;

    async fn create_admin(db_context: &mut DbContext, name: &str) -> Uuid {
        db_context
            .admins()
            .create()
            .name(name)
            .password_hash("hash")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn admin_names(db_context: &mut DbContext) -> Vec<String> {
        let mut names = db_context
            .admins()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.name)
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn dependencies(db_context: &DbContext, lock: &RecordingLock) -> Dependencies {
        Dependencies {
            db_context: db_context.clone(),
            acquire_lock: lock.port(),
        }
    }

    #[tokio::test]
    async fn deleting_the_last_admin_fails_and_deletes_nothing() {
        let mut db_context = mock_db_context().await;
        let maya = create_admin(&mut db_context, "Maya").await;

        let error = call(dependencies(&db_context, &RecordingLock::default()), maya)
            .await
            .expect_err("the last admin stays");

        assert!(error.find_source::<LastAdmin>().is_some(), "{error}");
        assert_eq!(admin_names(&mut db_context).await, ["Maya"]);
    }

    #[tokio::test]
    async fn deleting_one_of_two_admins_deletes_them_and_their_sessions() {
        let mut db_context = mock_db_context().await;
        let maya = create_admin(&mut db_context, "Maya").await;
        let ravi = create_admin(&mut db_context, "Ravi").await;
        for (admin_id, token) in [(maya, "maya"), (ravi, "ravi laptop"), (ravi, "ravi phone")] {
            db_context
                .admin_sessions()
                .create()
                .admin_id(admin_id)
                .token_hash(token.as_bytes().to_vec())
                .signed_in_at(Timestamp::UNIX_EPOCH)
                .last_used_at(Timestamp::UNIX_EPOCH)
                .exec(db_context.executor_mut())
                .await
                .unwrap();
        }

        call(dependencies(&db_context, &RecordingLock::default()), ravi)
            .await
            .unwrap();

        let sessions = db_context
            .admin_sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        assert_eq!(admin_names(&mut db_context).await, ["Maya"]);
        assert_eq!(sessions, [b"maya".to_vec()]);
    }

    #[tokio::test]
    async fn deleting_an_unknown_admin_succeeds_and_changes_nothing() {
        let mut db_context = mock_db_context().await;
        create_admin(&mut db_context, "Maya").await;

        call(
            dependencies(&db_context, &RecordingLock::default()),
            Uuid::new_v4(),
        )
        .await
        .unwrap();

        assert_eq!(admin_names(&mut db_context).await, ["Maya"]);
    }

    #[tokio::test]
    async fn deleting_an_admin_takes_the_admin_delete_lock() {
        let mut db_context = mock_db_context().await;
        create_admin(&mut db_context, "Maya").await;
        let ravi = create_admin(&mut db_context, "Ravi").await;
        let lock = RecordingLock::default();

        call(dependencies(&db_context, &lock), ravi).await.unwrap();

        assert_eq!(lock.keys(), ["admins:delete:v1"]);
    }
}
