use crate::Episode;
use crate::User;
use crate::WatchState;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
#[unique(user_id, episode_id)]
pub struct EpisodeWatchState {
    #[key]
    #[auto]
    pub id: Uuid,
    pub user_id: Uuid,
    #[belongs_to(key = user_id, references = id)]
    pub user: Deferred<User>,
    #[index]
    pub episode_id: Uuid,
    #[belongs_to(key = episode_id, references = id)]
    pub episode: Deferred<Episode>,
    pub resume_position_seconds: Option<i64>,
    pub watched: bool,
    pub watched_at: Option<Timestamp>,
    pub last_played: Option<Timestamp>,
}

impl EpisodeWatchState {
    pub fn watch_state(&self) -> WatchState {
        WatchState {
            resume_position_seconds: self.resume_position_seconds,
            watched: self.watched,
            watched_at: self.watched_at,
            last_played: self.last_played,
        }
    }
}
