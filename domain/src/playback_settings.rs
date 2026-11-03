use crate::User;
use toasty::Deferred;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct PlaybackSettings {
    #[key]
    pub user_id: Uuid,
    #[belongs_to]
    pub user: Deferred<User>,
    #[default(true)]
    pub autoplay_next_episode: bool,
    #[default(false)]
    pub ask_if_still_watching: bool,
    pub audio_language: Option<String>,
    pub subtitle_language: Option<String>,
}
