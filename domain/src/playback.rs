use crate::PlayMethod;
use jiff::SignedDuration;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct Playback {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub user_id: Uuid,
    #[index]
    pub session_id: Uuid,
    pub media_file_id: Uuid,
    pub method: PlayMethod,
    pub position: f64,
    pub paused: bool,
    pub started_at: Timestamp,
    pub last_reported_at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

impl Playback {
    const SILENCE_LIMIT: SignedDuration = SignedDuration::from_secs(60);
    const KEPT_FOR: SignedDuration = SignedDuration::from_hours(30 * 24);

    pub fn open_filter() -> toasty::stmt::Expr<bool> {
        Self::fields().ended_at().is_none()
    }

    pub fn silent_filter(now: Timestamp) -> toasty::stmt::Expr<bool> {
        Self::open_filter().and(
            Self::fields()
                .last_reported_at()
                .le(now - Self::SILENCE_LIMIT),
        )
    }

    pub fn expired_filter(now: Timestamp) -> toasty::stmt::Expr<bool> {
        Self::fields().ended_at().le(Some(now - Self::KEPT_FOR))
    }

    pub fn ended(&self) -> bool {
        self.ended_at.is_some()
    }
}
