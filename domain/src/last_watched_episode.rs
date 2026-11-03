use crate::Episode;
use crate::EpisodeWatchState;
use crate::Season;

pub fn last_watched_episode<'a>(
    episodes: impl IntoIterator<Item = (&'a Season, &'a Episode, &'a EpisodeWatchState)>,
) -> Option<(&'a Season, &'a Episode)> {
    episodes
        .into_iter()
        .filter_map(|(season, episode, state)| Some((state.watched_at?, season, episode)))
        .max_by_key(|(watched_at, season, episode)| {
            (*watched_at, season.listing_order(), episode.number)
        })
        .map(|(_, season, episode)| (season, episode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use toasty::Deferred;
    use uuid::Uuid;

    const EARLIER: &str = "2026-10-01T20:00:00Z";
    const LATER: &str = "2026-11-03T21:00:00Z";

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

    fn episode(number: i64) -> Episode {
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
            in_library: true,
            rating: None,
            nfo_modified_at: None,
            artwork: Deferred::default(),
        }
    }

    fn watched_at(timestamp: Option<&str>) -> EpisodeWatchState {
        EpisodeWatchState {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            user: Deferred::default(),
            episode_id: Uuid::nil(),
            episode: Deferred::default(),
            resume_position_seconds: None,
            watched: timestamp.is_some(),
            watched_at: timestamp.map(|x| x.parse::<Timestamp>().unwrap()),
            last_played: None,
        }
    }

    fn numbers(last: Option<(&Season, &Episode)>) -> Option<(i64, i64)> {
        last.map(|(season, episode)| (season.number, episode.number))
    }

    #[test]
    fn the_last_watched_episode_has_the_latest_watched_at() {
        let one = season(1);
        let episodes = [episode(1), episode(2), episode(3)];
        let states = [
            watched_at(Some(EARLIER)),
            watched_at(Some(LATER)),
            watched_at(Some(EARLIER)),
        ];

        let last = last_watched_episode(
            episodes
                .iter()
                .zip(&states)
                .map(|(episode, state)| (&one, episode, state)),
        );

        assert_eq!(numbers(last), Some((1, 2)));
    }

    #[test]
    fn episodes_watched_at_the_same_time_go_to_the_latest_in_season_and_episode_order() {
        let (one, two) = (season(1), season(2));
        let episodes = [episode(1), episode(2), episode(1)];
        let state = watched_at(Some(LATER));

        let last = last_watched_episode([
            (&two, &episodes[2], &state),
            (&one, &episodes[1], &state),
            (&one, &episodes[0], &state),
        ]);

        assert_eq!(numbers(last), Some((2, 1)));
    }

    #[test]
    fn an_unwatched_episode_is_never_the_last_watched() {
        let one = season(1);
        let episodes = [episode(1), episode(2)];
        let (watched, unwatched) = (watched_at(Some(EARLIER)), watched_at(None));

        let last = last_watched_episode([
            (&one, &episodes[0], &watched),
            (&one, &episodes[1], &unwatched),
        ]);

        assert_eq!(numbers(last), Some((1, 1)));
    }
}
