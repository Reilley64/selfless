use crate::Episode;
use crate::EpisodeWatchState;
use crate::Movie;
use crate::MovieWatchState;
use crate::Season;
use crate::episode_to_resume;
use crate::next_up;
use std::cmp::Reverse;

#[derive(Debug)]
pub enum ContinueWatching<'a> {
    Movie(&'a Movie, &'a MovieWatchState),
    Resume(&'a Season, &'a Episode, &'a EpisodeWatchState),
    NextUp(&'a Season, &'a Episode),
}

pub fn continue_watching<'a>(
    movies: impl IntoIterator<Item = (&'a Movie, &'a MovieWatchState)>,
    shows: impl IntoIterator<Item = Vec<(&'a Season, &'a Episode, Option<&'a EpisodeWatchState>)>>,
) -> Vec<ContinueWatching<'a>> {
    let shows = shows.into_iter().filter_map(|episodes| {
        let last_played = episodes
            .iter()
            .filter_map(|(_, _, state)| state.and_then(|x| x.last_played))
            .max();
        if let Some((season, episode, state)) = episode_to_resume(episodes.iter().copied()) {
            return Some((
                last_played,
                ContinueWatching::Resume(season, episode, state),
            ));
        }
        let (season, episode) = next_up(episodes)?;
        Some((
            last_played.max(episode.added_at),
            ContinueWatching::NextUp(season, episode),
        ))
    });
    let mut tiles: Vec<_> = movies
        .into_iter()
        .filter(|(movie, state)| movie.in_library && state.resume_position_seconds.is_some())
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
    use crate::test::movie_watch_state;
    use crate::test::resuming_at;
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
                ContinueWatching::Resume(season, episode, _) => {
                    Tile::Resume(season.show_id, season.number, episode.number)
                }
                ContinueWatching::NextUp(season, episode) => {
                    Tile::NextUp(season.show_id, season.number, episode.number)
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
        let (arrival, dune, hidden) = (movie(true), movie(true), movie(false));
        let states = [
            movie_watch_state(&arrival, Some(600), LATER),
            movie_watch_state(&dune, None, LATER),
            movie_watch_state(&hidden, Some(600), EARLIER),
        ];

        let found = continue_watching(
            [
                (&arrival, &states[0]),
                (&dune, &states[1]),
                (&hidden, &states[2]),
            ],
            [],
        );

        assert_eq!(tiles(found), [Tile::Movie(arrival.id)]);
    }
}
