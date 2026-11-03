use crate::AudioTranscodeCodec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioTranscode {
    pub codec: AudioTranscodeCodec,
    pub channels: i64,
}
