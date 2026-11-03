use crate::PlayReasonKind;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct PlaybackReason {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub playback_id: Uuid,
    pub index: i64,
    pub kind: PlayReasonKind,
    pub value: Option<String>,
}
