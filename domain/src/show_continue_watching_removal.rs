use crate::Show;
use crate::User;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[unique(user_id, show_id)]
pub struct ShowContinueWatchingRemoval {
    #[key]
    #[auto]
    pub id: Uuid,
    pub user_id: Uuid,
    #[belongs_to(key = user_id, references = id)]
    pub user: Deferred<User>,
    pub show_id: Uuid,
    #[belongs_to(key = show_id, references = id)]
    pub show: Deferred<Show>,
    pub removed_at: Timestamp,
}
