mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Session;
use domain::User;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    transaction
        .users()
        .filter(User::fields().id().eq(id))
        .delete()
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction
        .sessions()
        .filter(Session::fields().user_id().eq(id))
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
    use crate::test::mock_db_context;
    use crate::users::create_user;
    use domain::ClientKind;
    use domain::EmailAddress;
    use domain::Password;
    use jiff::Timestamp;

    const PASSWORD: &str = "a long passphrase";

    async fn create_user(db_context: &DbContext, name: &str) -> Uuid {
        create_user::call(
            create_user::Dependencies {
                db_context: db_context.clone(),
            },
            name.to_owned(),
            Password::new(PASSWORD.to_owned()).unwrap(),
            EmailAddress::new(format!("{name}@example.com")).unwrap(),
        )
        .await
        .unwrap()
        .id
    }

    async fn add_session(db_context: &mut DbContext, user_id: Uuid, device_name: &str) {
        db_context
            .sessions()
            .create()
            .user_id(user_id)
            .client_id(Uuid::new_v4())
            .client_kind(ClientKind::Web)
            .device_name(device_name)
            .token_hash(device_name.as_bytes().to_vec())
            .signed_in_at(Timestamp::UNIX_EPOCH)
            .last_used_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn deleting_a_user_ends_their_sessions() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&db_context, "Noor").await;
        let ravi = create_user(&db_context, "Ravi").await;
        add_session(&mut db_context, noor, "Noor's laptop").await;
        add_session(&mut db_context, noor, "Noor's phone").await;
        add_session(&mut db_context, ravi, "Ravi's laptop").await;

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            noor,
        )
        .await
        .unwrap();

        let users = db_context
            .users()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.id)
            .collect::<Vec<_>>();
        let sessions = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.device_name)
            .collect::<Vec<_>>();
        assert_eq!(users, [ravi]);
        assert_eq!(sessions, ["Ravi's laptop"]);
    }
}
