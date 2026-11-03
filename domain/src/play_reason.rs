use crate::HdrType;
use crate::PlayReasonKind;

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
    pub fn kind(&self) -> PlayReasonKind {
        match self {
            Self::Container(_) => PlayReasonKind::Container,
            Self::VideoCodec(_) => PlayReasonKind::VideoCodec,
            Self::VideoProfile(_) => PlayReasonKind::VideoProfile,
            Self::VideoLevel(_) => PlayReasonKind::VideoLevel,
            Self::VideoSize { .. } => PlayReasonKind::VideoSize,
            Self::VideoBitDepth(_) => PlayReasonKind::VideoBitDepth,
            Self::VideoRange(_) => PlayReasonKind::VideoRange,
            Self::AudioCodec(_) => PlayReasonKind::AudioCodec,
            Self::AudioChannels(_) => PlayReasonKind::AudioChannels,
            Self::Subtitle(_) => PlayReasonKind::Subtitle,
            Self::NoKeyframeIndex => PlayReasonKind::NoKeyframeIndex,
            Self::ForcedHls => PlayReasonKind::ForcedHls,
        }
    }

    pub fn value(&self) -> Option<String> {
        match self {
            Self::Container(x)
            | Self::VideoCodec(x)
            | Self::VideoProfile(x)
            | Self::AudioCodec(x)
            | Self::Subtitle(x) => Some(x.clone()),
            Self::VideoLevel(x) | Self::VideoBitDepth(x) | Self::AudioChannels(x) => {
                Some(x.to_string())
            }
            Self::VideoSize { width, height } => Some(format!("{width}x{height}")),
            Self::VideoRange(x) => Some(
                match x {
                    HdrType::Sdr => "sdr",
                    HdrType::Hdr10 => "hdr10",
                    HdrType::Hlg => "hlg",
                    HdrType::DolbyVision => "dolbyVision",
                }
                .to_owned(),
            ),
            Self::NoKeyframeIndex | Self::ForcedHls => None,
        }
    }

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

    #[test]
    fn a_reason_reads_as_the_value_the_profile_does_not_cover() {
        let values = [
            PlayReason::VideoLevel(153),
            PlayReason::VideoSize {
                width: 3840,
                height: 1600,
            },
            PlayReason::VideoBitDepth(10),
            PlayReason::VideoRange(HdrType::DolbyVision),
            PlayReason::AudioChannels(6),
            PlayReason::NoKeyframeIndex,
        ]
        .map(|x| x.value());

        assert_eq!(
            values,
            [
                Some("153".to_owned()),
                Some("3840x1600".to_owned()),
                Some("10".to_owned()),
                Some("dolbyVision".to_owned()),
                Some("6".to_owned()),
                None,
            ]
        );
    }
}
