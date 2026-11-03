use crate::ReportOutcome;
use jiff::SignedDuration;
use jiff::Timestamp;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WatchState {
    pub resume_position_seconds: Option<i64>,
    pub watched: bool,
    pub watched_at: Option<Timestamp>,
    pub last_played: Option<Timestamp>,
}

impl WatchState {
    pub fn after_report(
        &self,
        position: SignedDuration,
        duration: SignedDuration,
        reported_at: Timestamp,
    ) -> Option<WatchState> {
        match ReportOutcome::of(position, duration) {
            ReportOutcome::Nothing => None,
            ReportOutcome::ResumePosition => Some(WatchState {
                resume_position_seconds: Some(position.as_secs()),
                last_played: Some(reported_at),
                ..self.clone()
            }),
            ReportOutcome::Watched => Some(WatchState {
                resume_position_seconds: None,
                watched: true,
                watched_at: if self.watched {
                    self.watched_at
                } else {
                    Some(reported_at)
                },
                last_played: Some(reported_at),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EARLIER: &str = "2026-10-01T20:00:00Z";
    const REPORTED_AT: &str = "2026-11-03T21:00:00Z";
    const DURATION: SignedDuration = SignedDuration::from_secs(100 * 60);

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn report(state: &WatchState, position: SignedDuration) -> Option<WatchState> {
        state.after_report(position, DURATION, at(REPORTED_AT))
    }

    fn resuming_at(seconds: i64) -> WatchState {
        WatchState {
            resume_position_seconds: Some(seconds),
            watched: false,
            watched_at: None,
            last_played: Some(at(EARLIER)),
        }
    }

    #[test]
    fn a_position_under_the_first_minute_changes_nothing() {
        let state = resuming_at(1200);

        assert_eq!(report(&state, SignedDuration::from_millis(59_999)), None);
        assert_eq!(report(&WatchState::default(), SignedDuration::ZERO), None);
    }
    #[test]
    fn a_position_from_the_first_minute_sets_the_resume_position_and_last_played() {
        assert_eq!(
            report(&WatchState::default(), SignedDuration::from_secs(60)),
            Some(WatchState {
                resume_position_seconds: Some(60),
                watched: false,
                watched_at: None,
                last_played: Some(at(REPORTED_AT)),
            })
        );
    }

    #[test]
    fn a_position_just_under_ninety_percent_sets_the_resume_position() {
        let position = SignedDuration::from_secs(90 * 60) - SignedDuration::from_millis(1);

        assert_eq!(
            report(&resuming_at(1200), position).map(|x| x.resume_position_seconds),
            Some(Some(5399))
        );
    }

    #[test]
    fn seeking_back_sets_the_earlier_position() {
        assert_eq!(
            report(&resuming_at(3000), SignedDuration::from_secs(600))
                .map(|x| x.resume_position_seconds),
            Some(Some(600))
        );
    }
    #[test]
    fn ninety_percent_makes_an_unwatched_item_watched_and_clears_the_resume_position() {
        assert_eq!(
            report(&resuming_at(1200), SignedDuration::from_secs(90 * 60)),
            Some(WatchState {
                resume_position_seconds: None,
                watched: true,
                watched_at: Some(at(REPORTED_AT)),
                last_played: Some(at(REPORTED_AT)),
            })
        );
    }

    #[test]
    fn a_watched_item_reaching_ninety_percent_again_keeps_its_watched_at() {
        let watched = WatchState {
            resume_position_seconds: Some(1200),
            watched: true,
            watched_at: Some(at(EARLIER)),
            last_played: Some(at(EARLIER)),
        };

        assert_eq!(
            report(&watched, DURATION),
            Some(WatchState {
                resume_position_seconds: None,
                watched: true,
                watched_at: Some(at(EARLIER)),
                last_played: Some(at(REPORTED_AT)),
            })
        );
    }

    #[test]
    fn rewatching_a_watched_item_sets_a_resume_position_and_keeps_it_watched() {
        let watched = WatchState {
            resume_position_seconds: None,
            watched: true,
            watched_at: Some(at(EARLIER)),
            last_played: Some(at(EARLIER)),
        };

        assert_eq!(
            report(&watched, SignedDuration::from_secs(600)),
            Some(WatchState {
                resume_position_seconds: Some(600),
                watched: true,
                watched_at: Some(at(EARLIER)),
                last_played: Some(at(REPORTED_AT)),
            })
        );
    }
}
