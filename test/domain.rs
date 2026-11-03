use crate::Episode;
use crate::EpisodeWatchState;
use crate::Season;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

pub(crate) const EARLIER: &str = "2026-10-01T20:00:00Z";
pub(crate) const LATER: &str = "2026-11-03T21:00:00Z";
pub(crate) const LATEST: &str = "2026-11-04T21:00:00Z";

fn season(number: i64) -> Season {
    Season {
        id: Uuid::new_v4(),
        show_id: Uuid::nil(),
        show: Deferred::default(),
        number,
        episodes: Deferred::default(),
        artwork: Deferred::default(),
    }
}

fn episode(number: i64, in_library: bool) -> Episode {
    Episode {
        id: Uuid::new_v4(),
        season_id: Uuid::nil(),
        season: Deferred::default(),
        tvdb_id: crate::TvdbId(0),
        number,
        title: String::new(),
        air_date: None,
        overview: None,
        added_at: None,
        media_file_id: None,
        media_file: Deferred::default(),
        in_library,
        rating: None,
        nfo_modified_at: None,
        artwork: Deferred::default(),
    }
}

pub(crate) fn watch_state(
    resume_position_seconds: Option<i64>,
    watched_at: Option<&str>,
    last_played: &str,
) -> EpisodeWatchState {
    EpisodeWatchState {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        user: Deferred::default(),
        episode_id: Uuid::nil(),
        episode: Deferred::default(),
        resume_position_seconds,
        watched: watched_at.is_some(),
        watched_at: watched_at.map(|x| x.parse::<Timestamp>().unwrap()),
        last_played: Some(last_played.parse().unwrap()),
    }
}

pub(crate) fn watched_at(timestamp: &str) -> EpisodeWatchState {
    watch_state(None, Some(timestamp), timestamp)
}

pub(crate) fn resuming_at(seconds: i64, last_played: &str) -> EpisodeWatchState {
    watch_state(Some(seconds), None, last_played)
}

pub(crate) struct ShowEpisodes {
    seasons: Vec<Season>,
    episodes: Vec<(usize, Episode, Option<EpisodeWatchState>)>,
}

impl ShowEpisodes {
    pub(crate) fn new(episodes: Vec<(i64, i64, bool, Option<EpisodeWatchState>)>) -> Self {
        let mut seasons: Vec<Season> = Vec::new();
        let episodes = episodes
            .into_iter()
            .map(|(season_number, number, in_library, state)| {
                let index = match seasons.iter().position(|x| x.number == season_number) {
                    Some(index) => index,
                    None => {
                        seasons.push(season(season_number));
                        seasons.len() - 1
                    }
                };
                (index, episode(number, in_library), state)
            })
            .collect();
        Self { seasons, episodes }
    }

    pub(crate) fn episodes(&self) -> Vec<(&Season, &Episode, Option<&EpisodeWatchState>)> {
        self.episodes
            .iter()
            .map(|(index, episode, state)| (&self.seasons[*index], episode, state.as_ref()))
            .collect()
    }
}

pub(crate) fn numbers(found: Option<(&Season, &Episode)>) -> Option<(i64, i64)> {
    found.map(|(season, episode)| (season.number, episode.number))
}
