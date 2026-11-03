#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayReasonKind {
    Container,
    VideoCodec,
    VideoProfile,
    VideoLevel,
    VideoSize,
    VideoBitDepth,
    VideoRange,
    AudioCodec,
    AudioChannels,
    Subtitle,
    NoKeyframeIndex,
    ForcedHls,
}
