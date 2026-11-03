use crate::AudioStream;
use crate::SubtitleTrack;

#[derive(Clone, Copy, Debug, Default)]
pub struct TrackSelection<'a> {
    pub audio: Option<&'a AudioStream>,
    pub subtitle: Option<SubtitleTrack<'a>>,
}
