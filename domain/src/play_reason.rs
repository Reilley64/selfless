use crate::HdrType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayReason {
    Container(String),
    VideoCodec(String),
    VideoProfile(String),
    VideoSize { width: i64, height: i64 },
    VideoRange(HdrType),
    AudioCodec(String),
    AudioChannels(i64),
    NoKeyframeIndex,
    ForcedHls,
}

impl PlayReason {
    pub fn copies_video(&self) -> bool {
        matches!(
            self,
            Self::Container(_) | Self::AudioCodec(_) | Self::AudioChannels(_) | Self::ForcedHls
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrong_container_audio_or_forced_hls_leaves_the_video_to_copy() {
        let video_copied = [
            PlayReason::Container("mkv".to_owned()),
            PlayReason::VideoCodec("hevc".to_owned()),
            PlayReason::AudioCodec("dts".to_owned()),
            PlayReason::AudioChannels(6),
            PlayReason::NoKeyframeIndex,
            PlayReason::ForcedHls,
        ]
        .map(|x| x.copies_video());

        assert_eq!(video_copied, [true, false, true, true, false, true]);
    }
}
