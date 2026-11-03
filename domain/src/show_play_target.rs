use crate::Episode;
use crate::EpisodeWatchState;
use crate::PlayVerb;
use crate::Season;
use crate::episode_to_resume;
use crate::first_episode;
use crate::next_up;

#[derive(Debug)]
pub struct ShowPlayTarget<'a> {
    pub season: &'a Season,
    pub episode: &'a Episode,
    pub verb: PlayVerb,
}

pub fn show_play_target<'a>(
    episodes: impl IntoIterator<Item = (&'a Season, &'a Episode, Option<&'a EpisodeWatchState>)>,
) -> Option<ShowPlayTarget<'a>> {
    let episodes: Vec<_> = episodes.into_iter().collect();
    if let Some((season, episode, state)) = episode_to_resume(episodes.iter().copied()) {
        return Some(ShowPlayTarget {
            season,
            episode,
            verb: PlayVerb::Resume {
                position_seconds: state.resume_position_seconds?,
            },
        });
    }
    let (season, episode) = next_up(episodes.iter().copied()).or_else(|| {
        first_episode(
            episodes
                .iter()
                .filter(|(_, episode, _)| episode.in_library)
                .map(|&(season, episode, _)| (season, episode)),
        )
    })?;
    Some(ShowPlayTarget {
        season,
        episode,
        verb: PlayVerb::Play,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::EARLIER;
    use crate::test::LATER;
    use crate::test::ShowEpisodes;
    use crate::test::resuming_at;
    use crate::test::watched_at;

    fn target(show: &ShowEpisodes) -> Option<(i64, i64, PlayVerb)> {
        show_play_target(show.episodes()).map(|x| (x.season.number, x.episode.number, x.verb))
    }

    #[test]
    fn play_resumes_the_episode_with_a_resume_position_before_next_up() {
        let show = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(LATER))),
            (1, 2, true, None),
            (2, 4, true, Some(resuming_at(1534, EARLIER))),
        ]);

        assert_eq!(
            target(&show),
            Some((
                2,
                4,
                PlayVerb::Resume {
                    position_seconds: 1534
                }
            ))
        );
    }

    #[test]
    fn without_a_resume_position_play_plays_next_up() {
        let show = ShowEpisodes::new(vec![
            (2, 4, true, Some(watched_at(LATER))),
            (2, 5, true, None),
            (1, 1, true, None),
        ]);

        assert_eq!(target(&show), Some((2, 5, PlayVerb::Play)));
    }

    #[test]
    fn without_next_up_play_plays_the_first_episode() {
        let unwatched = ShowEpisodes::new(vec![
            (0, 1, true, None),
            (1, 2, true, None),
            (1, 1, true, None),
        ]);
        let caught_up = ShowEpisodes::new(vec![
            (1, 1, true, Some(watched_at(EARLIER))),
            (1, 2, true, Some(watched_at(LATER))),
        ]);

        assert_eq!(target(&unwatched), Some((1, 1, PlayVerb::Play)));
        assert_eq!(target(&caught_up), Some((1, 1, PlayVerb::Play)));
    }
}
