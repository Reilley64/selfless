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
    pub fn allows_remux(&self) -> bool {
        matches!(self, Self::Container(_) | Self::ForcedHls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_wrong_container_or_forced_hls_leaves_the_streams_to_copy() {
        let remuxable = [
            PlayReason::Container("mkv".to_owned()),
            PlayReason::VideoCodec("hevc".to_owned()),
            PlayReason::AudioCodec("dts".to_owned()),
            PlayReason::AudioChannels(6),
            PlayReason::NoKeyframeIndex,
            PlayReason::ForcedHls,
        ]
        .map(|x| x.allows_remux());

        assert_eq!(remuxable, [true, false, false, false, false, true]);
    }
}
