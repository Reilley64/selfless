use jiff::SignedDuration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportOutcome {
    Nothing,
    ResumePosition,
    Watched,
}

impl ReportOutcome {
    pub const START_THRESHOLD: SignedDuration = SignedDuration::from_secs(60);
    pub const WATCHED_THRESHOLD_PERCENT: i128 = 90;

    pub fn of(position: SignedDuration, duration: SignedDuration) -> Self {
        if position < Self::START_THRESHOLD {
            Self::Nothing
        } else if position.as_nanos() * 100 >= duration.as_nanos() * Self::WATCHED_THRESHOLD_PERCENT
        {
            Self::Watched
        } else {
            Self::ResumePosition
        }
    }
}
