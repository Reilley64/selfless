use crate::DbContext;
use crate::create_user;
use crate::ports::Clock;
use crate::session_tokens;
use domain::Invite;
use domain::User;
use std::fmt;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to accept an invite")]
pub struct Error;

#[derive(Debug)]
pub struct InviteNotFound;

impl fmt::Display for InviteNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("no pending invite has this token, or it has expired")
    }
}

impl std::error::Error for InviteNotFound {}

pub async fn call(
    dependencies: impl Into<Dependencies>,
    token: &str,
    name: String,
    password: String,
) -> Result<User, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let Some(invite) = Invite::filter_by_token_hash(session_tokens::hash(token))
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| now < x.expires_at)
    else {
        return Err(Error::caused_by(InviteNotFound));
    };
    let user = match create_user::call(
        create_user::Dependencies {
            db_context: db_context.clone(),
        },
        name,
        password,
        invite.email.clone(),
    )
    .await
    {
        Ok(user) => user,
        Err(error) => {
            let accepted = User::filter_by_email(invite.email.clone())
                .first()
                .exec(db_context.executor_mut())
                .await
                .map_err(Error::caused_by)?
                .is_some();
            return Err(if accepted {
                Error::caused_by(InviteNotFound)
            } else {
                Error::caused_by(error)
            });
        }
    };
    invite
        .delete()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(user)
}
