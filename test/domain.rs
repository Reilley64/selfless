use crate::Episode;
use crate::EpisodeWatchState;
use crate::Movie;
use crate::MovieContinueWatchingRemoval;
use crate::MovieWatchState;
use crate::Season;
use crate::Show;
use crate::ShowContinueWatchingRemoval;
use crate::ShowEpisode;
use crate::Title;
use crate::sort_key;
use jiff::Timestamp;
use toasty::Deferred;
use uuid::Uuid;

pub(crate) const EARLIER: &str = "2026-10-01T20:00:00Z";
pub(crate) const LATER: &str = "2026-11-03T21:00:00Z";
pub(crate) const LATEST: &str = "2026-11-04T21:00:00Z";

fn season(show_id: Uuid, number: i64) -> Season {
    Season {
        id: Uuid::new_v4(),
        show_id,
        show: Deferred::default(),
        number,
        episodes: Deferred::default(),
        artwork: Deferred::default(),
    }
}

pub(crate) fn episode(number: i64, in_library: bool) -> Episode {
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
    pub(crate) show_id: Uuid,
    seasons: Vec<Season>,
    episodes: Vec<(usize, Episode, Option<EpisodeWatchState>)>,
}

impl ShowEpisodes {
    pub(crate) fn new(episodes: Vec<(i64, i64, bool, Option<EpisodeWatchState>)>) -> Self {
        let show_id = Uuid::new_v4();
        let mut seasons: Vec<Season> = Vec::new();
        let episodes = episodes
            .into_iter()
            .map(|(season_number, number, in_library, state)| {
                let index = match seasons.iter().position(|x| x.number == season_number) {
                    Some(index) => index,
                    None => {
                        seasons.push(season(show_id, season_number));
                        seasons.len() - 1
                    }
                };
                (index, episode(number, in_library), state)
            })
            .collect();
        Self {
            show_id,
            seasons,
            episodes,
        }
    }

    pub(crate) fn added_at(mut self, season_number: i64, number: i64, added_at: &str) -> Self {
        for (index, episode, _) in &mut self.episodes {
            if (self.seasons[*index].number, episode.number) == (season_number, number) {
                episode.added_at = Some(added_at.parse().unwrap());
            }
        }
        self
    }

    pub(crate) fn episodes(&self) -> Vec<ShowEpisode<'_>> {
        self.episodes
            .iter()
            .map(|(index, episode, state)| ShowEpisode {
                season: &self.seasons[*index],
                episode,
                watch_state: state.as_ref(),
            })
            .collect()
    }
}

pub(crate) fn movie(in_library: bool) -> Movie {
    Movie {
        id: Uuid::new_v4(),
        tmdb_id: crate::TmdbId(0),
        radarr_id: crate::RadarrId(0),
        title: String::new(),
        year: 2016,
        overview: None,
        genres: Vec::new(),
        added_at: None,
        media_file_id: None,
        media_file: Deferred::default(),
        in_library,
        sort_title: None,
        original_title: None,
        runtime_minutes: None,
        certification: None,
        premiered: None,
        studio: None,
        rating: None,
        directors: Vec::new(),
        nfo_modified_at: None,
        actors: Deferred::default(),
        artwork: Deferred::default(),
        sync_problems: Deferred::default(),
    }
}

pub(crate) fn movie_watch_state(
    movie: &Movie,
    resume_position_seconds: Option<i64>,
    last_played: &str,
) -> MovieWatchState {
    MovieWatchState {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        user: Deferred::default(),
        movie_id: movie.id,
        movie: Deferred::default(),
        resume_position_seconds,
        watched: resume_position_seconds.is_none(),
        watched_at: None,
        last_played: Some(last_played.parse().unwrap()),
    }
}

pub(crate) fn movie_removal(movie: &Movie, removed_at: &str) -> MovieContinueWatchingRemoval {
    MovieContinueWatchingRemoval {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        user: Deferred::default(),
        movie_id: movie.id,
        movie: Deferred::default(),
        removed_at: removed_at.parse().unwrap(),
    }
}

pub(crate) fn show_removal(show_id: Uuid, removed_at: &str) -> ShowContinueWatchingRemoval {
    ShowContinueWatchingRemoval {
        id: Uuid::nil(),
        user_id: Uuid::nil(),
        user: Deferred::default(),
        show_id,
        show: Deferred::default(),
        removed_at: removed_at.parse().unwrap(),
    }
}

pub(crate) fn numbers(found: Option<ShowEpisode>) -> Option<(i64, i64)> {
    found.map(|x| (x.season.number, x.episode.number))
}

pub(crate) fn show(title: &str) -> Show {
    Show {
        id: Uuid::new_v4(),
        tvdb_id: crate::TvdbId(0),
        sonarr_id: crate::SonarrId(0),
        title: title.to_owned(),
        year: None,
        genres: Vec::new(),
        overview: None,
        sort_title: None,
        certification: None,
        premiered: None,
        end_date: None,
        status: None,
        network: None,
        rating: None,
        nfo_modified_at: None,
        seasons: Deferred::default(),
        actors: Deferred::default(),
        artwork: Deferred::default(),
        sync_problems: Deferred::default(),
    }
}

pub(crate) fn title(name: &str, genres: &[&str]) -> Title {
    Title {
        id: Uuid::new_v4(),
        genres: genres.iter().map(|x| (*x).to_owned()).collect(),
        sort_key: sort_key(None, name),
        added_at: None,
        released: None,
    }
}
