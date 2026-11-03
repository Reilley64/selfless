use crate::Movie;
use crate::MovieContinueWatchingRemoval;
use crate::MovieWatchState;
use crate::ShowContinueWatchingRemoval;
use crate::ShowEpisode;
use crate::episode_to_resume;
use crate::next_up;
use jiff::Timestamp;
use std::cmp::Reverse;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug)]
pub enum ContinueWatching<'a> {
    Movie(&'a Movie, &'a MovieWatchState),
    Resume(ShowEpisode<'a>),
    NextUp(ShowEpisode<'a>),
}

pub fn continue_watching<'a>(
    movies: impl IntoIterator<Item = (&'a Movie, &'a MovieWatchState)>,
    shows: impl IntoIterator<Item = Vec<ShowEpisode<'a>>>,
    movie_removals: &[MovieContinueWatchingRemoval],
    show_removals: &[ShowContinueWatchingRemoval],
) -> Vec<ContinueWatching<'a>> {
    let removed_at: HashMap<Uuid, Timestamp> = movie_removals
        .iter()
        .map(|x| (x.movie_id, x.removed_at))
        .chain(show_removals.iter().map(|x| (x.show_id, x.removed_at)))
        .collect();
    let removed = |title_id: Uuid, last_played: Option<Timestamp>| {
        removed_at
            .get(&title_id)
            .is_some_and(|removed_at| last_played.is_none_or(|x| *removed_at > x))
    };
    let shows = shows.into_iter().filter_map(|episodes| {
        let show_id = episodes.first()?.season.show_id;
        let last_played = episodes
            .iter()
            .filter_map(|x| x.watch_state?.last_played)
            .max();
        if removed(show_id, last_played) {
            return None;
        }
        if let Some(episode) = episode_to_resume(episodes.iter().copied()) {
            return Some((last_played, ContinueWatching::Resume(episode)));
        }
        let episode = next_up(episodes)?;
        Some((
            last_played.max(episode.episode.added_at),
            ContinueWatching::NextUp(episode),
        ))
    });
    let mut tiles: Vec<_> = movies
        .into_iter()
        .filter(|(movie, state)| {
            movie.in_library
                && state.resume_position_seconds.is_some()
                && !removed(movie.id, state.last_played)
        })
        .map(|(movie, state)| (state.last_played, ContinueWatching::Movie(movie, state)))
        .chain(shows)
        .collect();
    tiles.sort_by_key(|(sort_key, _)| Reverse(*sort_key));
    tiles.into_iter().map(|(_, tile)| tile).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::EARLIER;
    use crate::test::LATER;
    use crate::test::LATEST;
    use crate::test::ShowEpisodes;
    use crate::test::movie;
    use crate::test::movie_removal;
    use crate::test::movie_watch_state;
    use crate::test::resuming_at;
    use crate::test::show_removal;
    use crate::test::watched_at;
    use uuid::Uuid;

    #[derive(Debug, PartialEq)]
    enum Tile {
        Movie(Uuid),
        Resume(Uuid, i64, i64),
        NextUp(Uuid, i64, i64),
    }

    fn tiles(found: Vec<ContinueWatching>) -> Vec<Tile> {
        found
            .into_iter()
            .map(|x| match x {
                ContinueWatching::Movie(movie, _) => Tile::Movie(movie.id),
                ContinueWatching::Resume(x) => {
                    Tile::Resume(x.season.show_id, x.season.number, x.episode.number)
                }
                ContinueWatching::NextUp(x) => {
                    Tile::NextUp(x.season.show_id, x.season.number, x.episode.number)
                }
            })
            .collect()
    }

    #[test]
    fn a_show_appears_once_with_its_episode_to_resume_or_else_its_next_up_episode() {
        let resuming = ShowEpisodes::new(vec![
            (1, 1, true, Some(resuming_at(600, EARLIER))),
            (1, 2, true, Some(watched_at(LATER))),
            (1, 3, true, Some(resuming_at(300, LATER))),
            (1, 4, true, None),
        ]);
        let caught_up_to_next = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, None),
        ]);
        let caught_up = ShowEpisodes::new(vec![(1, 1, true, Some(watched_at(EARLIER)))]);

        let found = continue_watching(
            [],
            [
                resuming.episodes(),
                caught_up_to_next.episodes(),
                caught_up.episodes(),
            ],
            &[],
            &[],
        );

        assert_eq!(
            tiles(found),
            [
                Tile::Resume(resuming.show_id, 1, 3),
                Tile::NextUp(caught_up_to_next.show_id, 1, 2),
            ]
        );
    }

    #[test]
    fn tiles_sort_newest_first_by_the_last_play_of_the_title_or_the_added_time_of_a_next_up_episode()
     {
        let arrival = movie(true);
        let arrival_state = movie_watch_state(&arrival, Some(600), LATER);
        let resuming = ShowEpisodes::new(vec![
            (1, 1, true, Some(resuming_at(600, EARLIER))),
            (1, 2, true, Some(watched_at(LATEST))),
        ]);
        let new_episode = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, None),
        ])
        .added_at(1, 2, "2026-11-05T09:00:00Z");
        let old_episode = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at("2026-11-02T21:00:00Z"))),
            (1, 2, true, None),
        ])
        .added_at(1, 2, "2022-03-01T09:30:00Z");

        let found = continue_watching(
            [(&arrival, &arrival_state)],
            [
                old_episode.episodes(),
                resuming.episodes(),
                new_episode.episodes(),
            ],
            &[],
            &[],
        );

        assert_eq!(
            tiles(found),
            [
                Tile::NextUp(new_episode.show_id, 1, 2),
                Tile::Resume(resuming.show_id, 1, 1),
                Tile::Movie(arrival.id),
                Tile::NextUp(old_episode.show_id, 1, 2),
            ]
        );
    }

    #[test]
    fn a_movie_in_the_library_appears_while_the_user_has_a_resume_position_in_it() {
        let (arrival, dune, gone) = (movie(true), movie(true), movie(false));
        let states = [
            movie_watch_state(&arrival, Some(600), LATER),
            movie_watch_state(&dune, None, LATER),
            movie_watch_state(&gone, Some(600), EARLIER),
        ];

        let found = continue_watching(
            [
                (&arrival, &states[0]),
                (&dune, &states[1]),
                (&gone, &states[2]),
            ],
            [],
            &[],
            &[],
        );

        assert_eq!(tiles(found), [Tile::Movie(arrival.id)]);
    }

    #[test]
    fn a_removed_title_is_left_out_while_its_removal_is_newer_than_the_users_last_play_of_it() {
        let (arrival, dune) = (movie(true), movie(true));
        let states = [
            movie_watch_state(&arrival, Some(600), LATER),
            movie_watch_state(&dune, Some(600), LATER),
        ];
        let removed = ShowEpisodes::new(vec![(1, 1, true, Some(resuming_at(600, LATER)))]);
        let played_again = ShowEpisodes::new(vec![(1, 1, true, Some(resuming_at(600, LATEST)))]);

        let found = continue_watching(
            [(&arrival, &states[0]), (&dune, &states[1])],
            [removed.episodes(), played_again.episodes()],
            &[
                movie_removal(&arrival, LATEST),
                movie_removal(&dune, EARLIER),
            ],
            &[
                show_removal(removed.show_id, LATEST),
                show_removal(played_again.show_id, LATER),
            ],
        );

        assert_eq!(
            tiles(found),
            [
                Tile::Resume(played_again.show_id, 1, 1),
                Tile::Movie(dune.id)
            ]
        );
    }

    #[test]
    fn a_new_episode_does_not_bring_a_removed_show_back() {
        let andor = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, None),
        ])
        .added_at(1, 2, LATEST);

        let found = continue_watching(
            [],
            [andor.episodes()],
            &[],
            &[show_removal(andor.show_id, LATER)],
        );

        assert_eq!(tiles(found), []);
    }

    #[test]
    fn a_shows_last_played_counts_its_episodes_that_are_not_in_the_library() {
        let andor = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, false, Some(watched_at(LATEST))),
            (1, 3, true, None),
        ]);

        let found = continue_watching(
            [],
            [andor.episodes()],
            &[],
            &[show_removal(andor.show_id, LATER)],
        );

        assert_eq!(tiles(found), [Tile::NextUp(andor.show_id, 1, 3)]);
    }
}
