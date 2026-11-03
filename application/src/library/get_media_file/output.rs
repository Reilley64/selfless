use domain::HdrType;
use std::time::Duration;

#[derive(Debug)]
pub struct Output {
    pub media_file: Option<MediaFileDetails>,
}

#[derive(Debug)]
pub struct MediaFileDetails {
    pub path: String,
    pub container: Option<String>,
    pub duration: Option<Duration>,
    pub size: Option<i64>,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub subtitle_files: Vec<SubtitleFile>,
}

#[derive(Debug, PartialEq)]
pub struct VideoStream {
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: i64,
    pub height: i64,
    pub frame_rate: Option<f64>,
    pub hdr_type: HdrType,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AudioStream {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub channels: i64,
    pub title: Option<String>,
    pub default: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SubtitleStream {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub text: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SubtitleFile {
    pub path: String,
    pub language: Option<String>,
    pub hearing_impaired: bool,
    pub forced: bool,
}
