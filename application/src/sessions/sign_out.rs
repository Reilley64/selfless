mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use crate::tokens;
use domain::Session;

pub async fn call(dependencies: impl Into<Dependencies>, token: &str) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    db_context
        .sessions()
        .filter(Session::fields().token_hash().eq(tokens::hash(token)))
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
    use uuid::Uuid;

    async fn create_user(db_context: &mut DbContext, name: &str) -> Uuid {
        db_context
            .users()
            .create()
            .name(name)
            .email(EmailAddress::new(format!("{name}@example.com")).unwrap())
            .password_hash("hash")
            .created_at(jiff::Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    async fn add_session(db_context: &mut DbContext, user_id: Uuid, token: &str) {
        db_context
            .sessions()
            .create()
            .user_id(user_id)
            .client_id(Uuid::new_v4())
            .client_kind(ClientKind::Web)
            .device_name("Firefox on macOS")
            .token_hash(tokens::hash(token))
            .signed_in_at(Timestamp::UNIX_EPOCH)
            .last_used_at(Timestamp::UNIX_EPOCH)
            .exec(db_context.executor_mut())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn signing_out_ends_the_calling_session_only() {
        let mut db_context = mock_db_context().await;
        let noor = create_user(&mut db_context, "Noor").await;
        add_session(&mut db_context, noor, "calling token").await;
        add_session(&mut db_context, noor, "other token").await;

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            "calling token",
        )
        .await
        .unwrap();

        let token_hashes = db_context
            .sessions()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.token_hash)
            .collect::<Vec<_>>();
        assert_eq!(token_hashes, [tokens::hash("other token")]);
    }
}
