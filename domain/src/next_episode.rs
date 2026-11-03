use crate::Season;
use crate::ShowEpisode;

pub fn next_episode<'a>(
    episodes: impl IntoIterator<Item = ShowEpisode<'a>>,
    playing: impl IntoIterator<Item = ShowEpisode<'a>>,
) -> Option<ShowEpisode<'a>> {
    let last = playing
        .into_iter()
        .map(|x| (x.season.number, x.episode.number))
        .max()?;
    if Season::is_specials(last.0) {
        return None;
    }
    episodes
        .into_iter()
        .filter(|x| {
            x.episode.in_library
                && !Season::is_specials(x.season.number)
                && (x.season.number, x.episode.number) > last
        })
        .min_by_key(|x| (x.season.number, x.episode.number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::ShowEpisodes;
    use crate::test::numbers;

    fn after(show: &ShowEpisodes, playing: &[(i64, i64)]) -> Option<(i64, i64)> {
        numbers(next_episode(
            show.episodes(),
            show.episodes()
                .into_iter()
                .filter(|x| playing.contains(&(x.season.number, x.episode.number))),
        ))
    }

    #[test]
    fn the_next_episode_after_a_seasons_last_episode_is_the_first_of_the_next_season() {
        let show = ShowEpisodes::new(vec![
            (2, 2, true, None),
            (1, 1, true, None),
            (2, 1, true, None),
            (1, 2, true, None),
        ]);

        assert_eq!(after(&show, &[(1, 1)]), Some((1, 2)));
        assert_eq!(after(&show, &[(1, 2)]), Some((2, 1)));
    }

    #[test]
    fn the_next_episode_skips_specials_and_a_special_has_no_next_episode() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, None),
            (0, 1, true, None),
            (0, 2, true, None),
            (2, 1, true, None),
        ]);

        assert_eq!(after(&show, &[(1, 1)]), Some((2, 1)));
        assert_eq!(after(&show, &[(0, 1)]), None);
    }

    #[test]
    fn the_next_episode_after_a_two_episode_file_follows_its_second_episode() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, None),
            (1, 2, true, None),
            (1, 3, true, None),
            (1, 4, true, None),
        ]);

        assert_eq!(after(&show, &[(1, 2), (1, 3)]), Some((1, 4)));
    }

    #[test]
    fn the_next_episode_skips_episodes_not_in_the_library_and_the_last_one_has_none() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, None),
            (1, 2, false, None),
            (1, 3, true, None),
        ]);

        assert_eq!(after(&show, &[(1, 1)]), Some((1, 3)));
        assert_eq!(after(&show, &[(1, 3)]), None);
    }
}
