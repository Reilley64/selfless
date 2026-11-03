use crate::AudioCodecSupport;
use crate::AudioStream;
use crate::PlayReason;
use crate::VideoCodecSupport;
use crate::VideoStream;

#[derive(Clone, Debug, Default)]
pub struct CapabilityProfile {
    pub containers: Vec<String>,
    pub video_codecs: Vec<VideoCodecSupport>,
    pub audio_codecs: Vec<AudioCodecSupport>,
    pub subtitle_codecs: Vec<String>,
}

impl CapabilityProfile {
    pub fn direct_play_reasons(
        &self,
        container: &str,
        video: Option<&VideoStream>,
        audio: Option<&AudioStream>,
    ) -> Vec<PlayReason> {
        let mut reasons = Vec::new();
        if !self.containers.iter().any(|x| x == container) {
            reasons.push(PlayReason::Container(container.to_owned()));
        }
        if let Some(video) = video {
            match self.video_codecs.iter().find(|x| x.codec == video.codec) {
                None => reasons.push(PlayReason::VideoCodec(video.codec.clone())),
                Some(support) => reasons.extend(support.reasons(video)),
            }
        }
        if let Some(audio) = audio {
            match self.audio_codecs.iter().find(|x| x.codec == audio.codec) {
                None => reasons.push(PlayReason::AudioCodec(audio.codec.clone())),
                Some(support) if audio.channels > support.max_channels => {
                    reasons.push(PlayReason::AudioChannels(audio.channels));
                }
                Some(_) => {}
            }
        }
        reasons
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HdrType;
    use toasty::Deferred;
    use uuid::Uuid;

    fn browser() -> CapabilityProfile {
        CapabilityProfile {
            containers: vec!["mp4".to_owned(), "webm".to_owned()],
            video_codecs: vec![VideoCodecSupport {
                codec: "h264".to_owned(),
                profiles: vec!["main".to_owned(), "high".to_owned()],
                max_width: 1920,
                max_height: 1080,
                hdr_types: vec![HdrType::Sdr],
            }],
            audio_codecs: vec![AudioCodecSupport {
                codec: "aac".to_owned(),
                max_channels: 2,
            }],
            subtitle_codecs: Vec::new(),
        }
    }

    fn video(
        codec: &str,
        profile: &str,
        width: i64,
        height: i64,
        hdr_type: HdrType,
    ) -> VideoStream {
        VideoStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 0,
            codec: codec.to_owned(),
            profile: Some(profile.to_owned()),
            width,
            height,
            frame_rate: None,
            hdr_type,
        }
    }

    fn audio(codec: &str, channels: i64) -> AudioStream {
        AudioStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 1,
            codec: codec.to_owned(),
            language: None,
            channels,
            title: None,
            is_default: true,
        }
    }

    #[test]
    fn a_profile_that_covers_the_container_video_and_audio_plays_the_file_directly() {
        let reasons = browser().direct_play_reasons(
            "mp4",
            Some(&video("h264", "High", 1920, 800, HdrType::Sdr)),
            Some(&audio("aac", 2)),
        );

        assert_eq!(reasons, []);
    }

    #[test]
    fn each_part_the_profile_does_not_cover_is_a_reason() {
        let reasons = browser().direct_play_reasons(
            "mkv",
            Some(&video("h264", "High 10", 3840, 1600, HdrType::Hdr10)),
            Some(&audio("aac", 6)),
        );

        assert_eq!(
            reasons,
            [
                PlayReason::Container("mkv".to_owned()),
                PlayReason::VideoProfile("high10".to_owned()),
                PlayReason::VideoSize {
                    width: 3840,
                    height: 1600
                },
                PlayReason::VideoRange(HdrType::Hdr10),
                PlayReason::AudioChannels(6),
            ]
        );
    }

    #[test]
    fn a_codec_the_profile_does_not_list_is_a_reason() {
        let reasons = browser().direct_play_reasons(
            "mp4",
            Some(&video("hevc", "Main", 1920, 1080, HdrType::Sdr)),
            Some(&audio("eac3", 6)),
        );

        assert_eq!(
            reasons,
            [
                PlayReason::VideoCodec("hevc".to_owned()),
                PlayReason::AudioCodec("eac3".to_owned()),
            ]
        );
    }

    #[test]
    fn dolby_vision_plays_as_its_hdr10_base_layer_where_hdr10_plays() {
        let mut profile = browser();
        profile.video_codecs[0].hdr_types = vec![HdrType::Sdr, HdrType::Hdr10];

        let reasons = profile.direct_play_reasons(
            "mp4",
            Some(&video("h264", "High", 1920, 1080, HdrType::DolbyVision)),
            None,
        );

        assert_eq!(reasons, []);
    }
}
