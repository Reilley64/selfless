use crate::Episode;
use crate::EpisodeWatchState;
use crate::Season;
use crate::last_watched_episode;

pub fn next_up<'a>(
    episodes: impl IntoIterator<Item = (&'a Season, &'a Episode, Option<&'a EpisodeWatchState>)>,
) -> Option<(&'a Season, &'a Episode)> {
    let numbered: Vec<_> = episodes
        .into_iter()
        .filter(|(season, _, _)| !Season::is_specials(season.number))
        .collect();
    let (season, episode) = last_watched_episode(
        numbered
            .iter()
            .filter_map(|&(season, episode, state)| Some((season, episode, state?))),
    )?;
    let anchor = (season.number, episode.number);
    numbered
        .into_iter()
        .filter(|(season, episode, _)| {
            episode.in_library && (season.number, episode.number) > anchor
        })
        .min_by_key(|(season, episode, _)| (season.number, episode.number))
        .map(|(season, episode, _)| (season, episode))
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

    #[test]
    fn next_up_is_the_first_episode_after_the_last_watched_one_in_season_and_episode_order() {
        let show = ShowEpisodes::new(vec![
            (2, 1, true, None),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 1, true, Some(watched_at(EARLIER))),
            (2, 2, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), Some((2, 1)));
    }

    #[test]
    fn rewatching_an_episode_that_is_already_watched_does_not_move_next_up() {
        let show = ShowEpisodes::new(vec![
            (
                1,
                1,
                true,
                Some(watch_state(Some(600), Some(EARLIER), LATEST)),
            ),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 3, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), Some((1, 3)));
    }

    #[test]
    fn watching_a_special_does_not_change_next_up_and_a_special_is_never_next_up() {
        let show = ShowEpisodes::new(vec![
            (0, 1, true, Some(watched_at(LATEST))),
            (0, 2, true, None),
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, None),
        ]);
        let specials_watched = ShowEpisodes::new(vec![
            (0, 1, true, Some(watched_at(LATEST))),
            (0, 2, true, None),
            (1, 1, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), Some((1, 2)));
        assert_eq!(numbers(next_up(specials_watched.episodes())), None);
    }

    #[test]
    fn next_up_skips_an_episode_that_is_not_in_the_library() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(LATER))),
            (1, 2, false, None),
            (1, 3, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), Some((1, 3)));
    }

    #[test]
    fn a_show_watched_up_to_its_last_episode_in_the_library_has_no_next_up() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 3, false, None),
            (0, 1, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), None);
    }

    #[test]
    fn a_show_with_nothing_watched_has_no_next_up() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(resuming_at(600, LATER))),
            (1, 2, true, None),
        ]);

        assert_eq!(numbers(next_up(show.episodes())), None);
    }
}
