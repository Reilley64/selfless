use crate::Episode;
use crate::Season;

pub fn first_episode<'a>(
    episodes: impl IntoIterator<Item = (&'a Season, &'a Episode)>,
) -> Option<(&'a Season, &'a Episode)> {
    episodes
        .into_iter()
        .min_by_key(|(season, episode)| (season.listing_order(), episode.number))
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;
    use uuid::Uuid;

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
            tvdb_id: 0,
            number,
            title: String::new(),
            air_date: None,
            overview: None,
            added_at: None,
            media_file_id: None,
            media_file: Deferred::default(),
            visible: true,
            rating: None,
            nfo_modified_at: None,
            artwork: Deferred::default(),
        }
    }

    fn numbers(first: Option<(&Season, &Episode)>) -> Option<(i64, i64)> {
        first.map(|(season, episode)| (season.number, episode.number))
    }

    #[test]
    fn the_first_episode_is_the_first_of_the_first_numbered_season() {
        let (specials, one, two) = (season(0), season(1), season(2));
        let episodes = [episode(1), episode(3), episode(2), episode(1)];
        let seasons = [&specials, &one, &one, &two];

        let first = first_episode(seasons.into_iter().zip(&episodes));

        assert_eq!(numbers(first), Some((1, 2)));
    }

    #[test]
    fn a_show_with_only_specials_starts_at_its_first_special() {
        let specials = season(0);
        let episodes = [episode(2), episode(1)];

        let first = first_episode(episodes.iter().map(|x| (&specials, x)));

        assert_eq!(numbers(first), Some((0, 1)));
    }
}
