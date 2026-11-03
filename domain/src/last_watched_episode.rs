use crate::ShowEpisode;

pub fn last_watched_episode<'a>(
    episodes: impl IntoIterator<Item = ShowEpisode<'a>>,
) -> Option<ShowEpisode<'a>> {
    episodes
        .into_iter()
        .filter_map(|x| Some((x.watch_state?.watched_at?, x)))
        .max_by_key(|(watched_at, x)| (*watched_at, x.season.listing_order(), x.episode.number))
        .map(|(_, x)| x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::EARLIER;
    use crate::test::LATER;
    use crate::test::ShowEpisodes;
    use crate::test::numbers;
    use crate::test::resuming_at;
    use crate::test::watched_at;

    #[test]
    fn the_last_watched_episode_has_the_latest_watched_at() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 3, true, Some(watched_at(EARLIER))),
        ]);

        assert_eq!(numbers(last_watched_episode(show.episodes())), Some((1, 2)));
    }

    #[test]
    fn episodes_watched_at_the_same_time_go_to_the_latest_in_season_and_episode_order() {
        let show = ShowEpisodes::new(vec![
            (2, 1, true, Some(watched_at(LATER))),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 1, true, Some(watched_at(LATER))),
        ]);

        assert_eq!(numbers(last_watched_episode(show.episodes())), Some((2, 1)));
    }

    #[test]
    fn an_unwatched_episode_is_never_the_last_watched() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, Some(resuming_at(600, LATER))),
            (1, 3, true, None),
        ]);

        assert_eq!(numbers(last_watched_episode(show.episodes())), Some((1, 1)));
    }
}
