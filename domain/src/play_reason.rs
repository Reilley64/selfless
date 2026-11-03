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
}
