use crate::Movie;
use crate::User;
use crate::WatchState;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[unique(user_id, movie_id)]
pub struct MovieWatchState {
    #[key]
    #[auto]
    pub id: Uuid,
    pub user_id: Uuid,
    #[belongs_to(key = user_id, references = id)]
    pub user: Deferred<User>,
    #[index]
    pub movie_id: Uuid,
    #[belongs_to(key = movie_id, references = id)]
    pub movie: Deferred<Movie>,
    pub resume_position_seconds: Option<i64>,
    pub watched: bool,
    pub watched_at: Option<Timestamp>,
    pub last_played: Option<Timestamp>,
}

impl MovieWatchState {
    pub fn watch_state(&self) -> WatchState {
        WatchState {
            resume_position_seconds: self.resume_position_seconds,
            watched: self.watched,
            watched_at: self.watched_at,
            last_played: self.last_played,
        }
    }
}
