use crate::DbContext;
use crate::ports::Clock;
use domain::Session;
use domain::User;
use jiff::Timestamp;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub clock: Clock,
}

#[ohno::error]
#[display("failed to list users")]
pub struct Error;

pub struct ListedUser {
    pub user: User,
    pub signed_in_devices: usize,
    pub last_used_at: Option<Timestamp>,
}

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Vec<ListedUser>, Error> {
    let Dependencies {
        mut db_context,
        clock,
    } = dependencies.into();
    let now = clock.call();
    let mut users = User::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    users.sort_by_key(|x| x.name.to_lowercase());
    let sessions = Session::all()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .into_iter()
        .filter(|x| !x.expired(now))
        .collect::<Vec<_>>();
    Ok(users
        .into_iter()
        .map(|user| {
            let signed_in = sessions.iter().filter(|x| x.user_id == user.id);
            ListedUser {
                signed_in_devices: signed_in.clone().count(),
                last_used_at: signed_in.map(|x| x.last_used_at).max(),
                user,
            }
        })
        .collect())
}
