mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Session;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .sessions()
        .filter(Session::fields().id().eq(id))
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::mock_db_context;
    use domain::ClientKind;
    use domain::EmailAddress;
    use jiff::Timestamp;

    async fn create_user(db_context: &mut DbContext, name: &str) -> Uuid {
        db_context
            .users()
            .create()
            .name(name)
            .email(EmailAddress::new(format!("{name}@example.com")).unwrap())
            .password_hash("hash")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn add_session(db_context: &mut DbContext, user_id: Uuid, device_name: &str) -> Uuid {
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
            .unwrap()
            .id
    }

    async fn device_names(db_context: &mut DbContext) -> Vec<String> {
        let mut names = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.device_name)
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[tokio::test]
    async fn ending_one_session_signs_out_only_that_session() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&mut db_context, "Noor").await;
        let laptop = add_session(&mut db_context, noor, "Firefox on macOS").await;
        add_session(&mut db_context, noor, "Living Room").await;

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            laptop,
        )
        .await
        .unwrap();

        assert_eq!(device_names(&mut db_context).await, ["Living Room"]);
    }
}
