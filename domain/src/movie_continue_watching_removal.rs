use crate::Movie;
use crate::User;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[unique(user_id, movie_id)]
pub struct MovieContinueWatchingRemoval {
    #[key]
    #[auto]
    pub id: Uuid,
    pub user_id: Uuid,
    #[belongs_to(key = user_id, references = id)]
    pub user: Deferred<User>,
    pub movie_id: Uuid,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: Deferred<Movie>,
    pub removed_at: Timestamp,
}
