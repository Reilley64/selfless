use crate::HdrType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayReason {
    Container(String),
    VideoCodec(String),
    VideoProfile(String),
    VideoLevel(i64),
    VideoSize { width: i64, height: i64 },
    VideoBitDepth(i64),
    VideoRange(HdrType),
    AudioCodec(String),
    AudioChannels(i64),
    Subtitle(String),
    NoKeyframeIndex,
    ForcedHls,
}

impl PlayReason {
    pub fn transcodes_video(&self) -> bool {
        matches!(
            self,
            Self::VideoCodec(_)
                | Self::VideoProfile(_)
                | Self::VideoLevel(_)
                | Self::VideoSize { .. }
                | Self::VideoBitDepth(_)
                | Self::VideoRange(_)
                | Self::NoKeyframeIndex
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_video_stream_the_profile_does_not_cover_or_a_missing_keyframe_index_transcodes_the_video()
     {
        let transcodes = [
            PlayReason::Container("mkv".to_owned()),
            PlayReason::VideoCodec("hevc".to_owned()),
            PlayReason::VideoProfile("main10".to_owned()),
            PlayReason::VideoLevel(156),
            PlayReason::VideoSize {
                width: 3840,
                height: 2160,
            },
            PlayReason::VideoBitDepth(12),
            PlayReason::VideoRange(HdrType::Hdr10),
            PlayReason::AudioCodec("dts".to_owned()),
            PlayReason::AudioChannels(6),
            PlayReason::Subtitle("srt".to_owned()),
            PlayReason::NoKeyframeIndex,
            PlayReason::ForcedHls,
        ]
        .map(|x| x.transcodes_video());

        assert_eq!(
            transcodes,
            [
                false, true, true, true, true, true, true, false, false, false, true, false
            ]
        );
    }
}
