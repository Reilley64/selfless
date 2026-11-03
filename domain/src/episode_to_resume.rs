use crate::Episode;
use crate::EpisodeWatchState;
use crate::Season;

pub fn episode_to_resume<'a>(
    episodes: impl IntoIterator<Item = (&'a Season, &'a Episode, Option<&'a EpisodeWatchState>)>,
) -> Option<(&'a Season, &'a Episode, &'a EpisodeWatchState)> {
    episodes
        .into_iter()
        .filter_map(|(season, episode, state)| {
            let state = state.filter(|x| x.resume_position_seconds.is_some())?;
            episode.in_library.then_some((season, episode, state))
        })
        .max_by_key(|(_, _, state)| state.last_played)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::EARLIER;
    use crate::test::LATER;
    use crate::test::LATEST;
    use crate::test::ShowEpisodes;
    use crate::test::numbers;
    use crate::test::resuming_at;
    use crate::test::watch_state;
    use crate::test::watched_at;

    fn resume(show: &ShowEpisodes) -> Option<(i64, i64, Option<i64>)> {
        episode_to_resume(show.episodes()).map(|(season, episode, state)| {
            let (season, episode) = numbers(Some((season, episode))).unwrap();
            (season, episode, state.resume_position_seconds)
        })
    }

    #[test]
    fn the_episode_to_resume_is_the_one_with_a_resume_position_played_most_recently() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(resuming_at(600, EARLIER))),
            (0, 1, true, Some(resuming_at(300, LATER))),
            (1, 3, true, Some(watched_at(LATEST))),
            (1, 2, false, Some(resuming_at(900, LATEST))),
            (
                2,
                1,
                true,
                Some(watch_state(Some(120), Some(EARLIER), EARLIER)),
            ),
        ]);

        assert_eq!(resume(&show), Some((0, 1, Some(300))));
    }

    #[test]
    fn a_show_without_a_resume_position_has_no_episode_to_resume() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(LATER))),
            (1, 2, true, None),
        ]);

        assert_eq!(resume(&show), None);
    }
}
