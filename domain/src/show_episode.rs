use crate::Episode;
use crate::EpisodeWatchState;
use crate::Season;

#[derive(Clone, Copy, Debug)]
pub struct ShowEpisode<'a> {
    pub season: &'a Season,
    pub episode: &'a Episode,
    pub watch_state: Option<&'a EpisodeWatchState>,
}
