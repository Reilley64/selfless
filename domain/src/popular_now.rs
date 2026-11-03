use crate::EpisodeWatchState;
use crate::MovieWatchState;
use jiff::SignedDuration;
use jiff::Timestamp;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::HashSet;
use uuid::Uuid;

const POPULAR_NOW_WINDOW: SignedDuration = SignedDuration::from_hours(14 * 24);

pub fn popular_now_start(now: Timestamp) -> Timestamp {
    now - POPULAR_NOW_WINDOW
}

pub fn popular_now<'a>(
    movies: impl IntoIterator<Item = &'a MovieWatchState>,
    shows: impl IntoIterator<Item = (Uuid, &'a EpisodeWatchState)>,
    now: Timestamp,
) -> Vec<Uuid> {
    let start = popular_now_start(now);
    let mut plays = HashMap::<Uuid, (HashSet<Uuid>, Timestamp)>::new();
    let movie_plays = movies
        .into_iter()
        .map(|x| (x.movie_id, x.user_id, x.last_played));
    let show_plays = shows
        .into_iter()
        .map(|(show_id, x)| (show_id, x.user_id, x.last_played));
    for (title_id, user_id, last_played) in movie_plays.chain(show_plays) {
        let Some(last_played) = last_played.filter(|x| *x >= start) else {
            continue;
        };
        let (users, latest) = plays
            .entry(title_id)
            .or_insert_with(|| (HashSet::new(), last_played));
        users.insert(user_id);
        *latest = last_played.max(*latest);
    }
    let mut ranked: Vec<_> = plays
        .into_iter()
        .map(|(title_id, (users, latest))| (Reverse(users.len()), Reverse(latest), title_id))
        .collect();
    ranked.sort();
    ranked
        .into_iter()
        .map(|(_, _, title_id)| title_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::movie;
    use crate::test::movie_watch_state;
    use crate::test::watch_state;

    const NOW: &str = "2026-11-03T21:00:00Z";

    fn movie_play(movie_id: Uuid, user_id: Uuid, last_played: &str) -> MovieWatchState {
        let mut state = movie_watch_state(&movie(true), None, last_played);
        state.movie_id = movie_id;
        state.user_id = user_id;
        state
    }

    fn episode_play(user_id: Uuid, last_played: &str) -> EpisodeWatchState {
        let mut state = watch_state(Some(600), None, last_played);
        state.episode_id = Uuid::new_v4();
        state.user_id = user_id;
        state
    }

    fn ranked(movies: &[MovieWatchState], shows: &[(Uuid, EpisodeWatchState)]) -> Vec<Uuid> {
        popular_now(
            movies,
            shows.iter().map(|(show_id, state)| (*show_id, state)),
            NOW.parse().unwrap(),
        )
    }

    #[test]
    fn titles_rank_by_how_many_users_played_them_and_a_show_counts_once_per_user() {
        let [noor, sam, ines] = [(); 3].map(|()| Uuid::new_v4());
        let [arrival, dune, severance] = [(); 3].map(|()| Uuid::new_v4());

        let found = ranked(
            &[
                movie_play(arrival, noor, "2026-11-01T20:00:00Z"),
                movie_play(arrival, sam, "2026-11-02T20:00:00Z"),
                movie_play(arrival, ines, "2026-11-03T20:00:00Z"),
                movie_play(dune, noor, "2026-11-03T20:00:00Z"),
            ],
            &[
                (severance, episode_play(noor, "2026-11-01T20:00:00Z")),
                (severance, episode_play(noor, "2026-11-02T20:00:00Z")),
                (severance, episode_play(noor, "2026-11-03T20:00:00Z")),
                (severance, episode_play(sam, "2026-10-30T20:00:00Z")),
            ],
        );

        assert_eq!(found, [arrival, severance, dune]);
    }

    #[test]
    fn a_play_counts_from_exactly_14_days_before_now() {
        let noor = Uuid::new_v4();
        let [arrival, dune] = [(); 2].map(|()| Uuid::new_v4());

        let found = ranked(
            &[
                movie_play(arrival, noor, "2026-10-20T21:00:00Z"),
                movie_play(dune, noor, "2026-10-20T20:59:59Z"),
            ],
            &[],
        );

        assert_eq!(found, [arrival]);
    }

    #[test]
    fn titles_played_by_as_many_users_rank_by_their_most_recent_play() {
        let [noor, sam] = [(); 2].map(|()| Uuid::new_v4());
        let [arrival, severance] = [(); 2].map(|()| Uuid::new_v4());

        let found = ranked(
            &[movie_play(arrival, noor, "2026-11-02T20:00:00Z")],
            &[
                (severance, episode_play(sam, "2026-11-01T20:00:00Z")),
                (severance, episode_play(sam, "2026-11-03T20:00:00Z")),
            ],
        );

        assert_eq!(found, [severance, arrival]);
    }

    #[test]
    fn a_title_nobody_played_is_left_out() {
        let mut marked_watched = movie_play(Uuid::new_v4(), Uuid::new_v4(), NOW);
        marked_watched.last_played = None;

        assert!(ranked(&[marked_watched], &[]).is_empty());
    }
}
