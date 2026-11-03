#[derive(Clone, Copy, Debug, PartialEq, Eq, toasty::Embed)]
#[column(type = text)]
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
