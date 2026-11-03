use crate::AudioStream;
use crate::AudioTranscode;
use crate::CapabilityProfile;
use crate::PlayMethod;
use crate::PlayReason;
use crate::VideoStream;
use crate::VideoTranscode;
use crate::usable_keyframe_index;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayDecision {
    pub method: PlayMethod,
    pub reasons: Vec<PlayReason>,
    pub video_transcode: Option<VideoTranscode>,
    pub audio_transcodes: Vec<Option<AudioTranscode>>,
}

impl PlayDecision {
    pub fn new(
        profile: &CapabilityProfile,
        mut reasons: Vec<PlayReason>,
        video: &VideoStream,
        keyframes: &[Duration],
        audio_streams: &[&AudioStream],
    ) -> Self {
        if reasons.is_empty() {
            return Self {
                method: PlayMethod::DirectPlay,
                reasons,
                video_transcode: None,
                audio_transcodes: vec![None; audio_streams.len()],
            };
        }
        if !reasons.iter().any(PlayReason::transcodes_video) && !usable_keyframe_index(keyframes) {
            reasons.push(PlayReason::NoKeyframeIndex);
        }
        let video_transcode = reasons
            .iter()
            .any(PlayReason::transcodes_video)
            .then(|| video.transcode());
        let audio_transcodes = audio_streams
            .iter()
            .map(|stream| {
                let reason = profile.audio_reason(stream)?;
                if !reasons.contains(&reason) {
                    reasons.push(reason);
                }
                Some(profile.audio_transcode(stream))
            })
            .collect::<Vec<_>>();
        let method = if video_transcode.is_none() && audio_transcodes.iter().all(Option::is_none) {
            PlayMethod::Remux
        } else {
            PlayMethod::Transcode
        };
        Self {
            method,
            reasons,
            video_transcode,
            audio_transcodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AudioCodecSupport;
    use crate::AudioTranscodeCodec;
    use crate::HdrType;
    use crate::VideoCodecSupport;
    use toasty::Deferred;
    use uuid::Uuid;

    fn profile() -> CapabilityProfile {
        CapabilityProfile {
            containers: vec!["mp4".to_owned()],
            video_codecs: vec![VideoCodecSupport {
                codec: "h264".to_owned(),
                profiles: vec!["high".to_owned()],
                max_level: 51,
                max_width: 1920,
                max_height: 1080,
                bit_depths: vec![8],
                hdr_types: vec![HdrType::Sdr],
                dolby_vision_profiles: Vec::new(),
            }],
            audio_codecs: vec![AudioCodecSupport {
                codec: "aac".to_owned(),
                max_channels: 2,
            }],
            subtitle_codecs: Vec::new(),
        }
    }

    fn video() -> VideoStream {
        VideoStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 0,
            codec: "h264".to_owned(),
            profile: Some("High".to_owned()),
            level: None,
            bit_depth: None,
            width: 1920,
            height: 800,
            frame_rate: None,
            hdr_type: HdrType::Sdr,
            dolby_vision_profile: None,
            dolby_vision_compatibility_id: None,
            interlaced: false,
            bit_rate: None,
        }
    }

    fn audio(index: i64, codec: &str) -> AudioStream {
        AudioStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index,
            codec: codec.to_owned(),
            language: None,
            channels: 2,
            title: None,
            is_default: index == 1,
        }
    }

    fn keyframes() -> Vec<Duration> {
        (0..10).map(|x| Duration::from_secs(x * 6)).collect()
    }

    fn container() -> PlayReason {
        PlayReason::Container("mkv".to_owned())
    }

    #[test]
    fn without_reasons_the_file_plays_directly_and_nothing_is_converted() {
        let audio = audio(1, "aac");

        let decision = PlayDecision::new(&profile(), Vec::new(), &video(), &[], &[&audio]);

        assert_eq!(
            decision,
            PlayDecision {
                method: PlayMethod::DirectPlay,
                reasons: Vec::new(),
                video_transcode: None,
                audio_transcodes: vec![None],
            }
        );
    }

    #[test]
    fn a_reason_that_converts_no_stream_remuxes_with_a_usable_keyframe_index() {
        let audio = audio(1, "aac");

        let decision = PlayDecision::new(
            &profile(),
            vec![container()],
            &video(),
            &keyframes(),
            &[&audio],
        );

        assert_eq!(
            (decision.method, decision.reasons, decision.video_transcode),
            (PlayMethod::Remux, vec![container()], None)
        );
    }

    #[test]
    fn without_a_usable_keyframe_index_the_video_is_transcoded_with_a_no_keyframe_index_reason() {
        let decision = PlayDecision::new(&profile(), vec![container()], &video(), &[], &[]);

        assert_eq!(
            (decision.method, decision.reasons),
            (
                PlayMethod::Transcode,
                vec![container(), PlayReason::NoKeyframeIndex]
            )
        );
        assert_eq!(decision.video_transcode, Some(video().transcode()));
    }

    #[test]
    fn a_video_reason_transcodes_the_video_whatever_its_keyframes() {
        let reasons = vec![PlayReason::VideoCodec("hevc".to_owned())];

        let decision = PlayDecision::new(&profile(), reasons.clone(), &video(), &[], &[]);

        assert_eq!(
            (decision.method, decision.reasons),
            (PlayMethod::Transcode, reasons)
        );
        assert!(decision.video_transcode.is_some());
    }

    #[test]
    fn each_audio_track_the_profile_does_not_cover_is_transcoded_with_its_reason_once() {
        let streams = [audio(1, "dts"), audio(2, "aac"), audio(3, "dts")];
        let reasons = vec![container(), PlayReason::AudioCodec("dts".to_owned())];

        let decision = PlayDecision::new(
            &profile(),
            reasons.clone(),
            &video(),
            &keyframes(),
            &streams.iter().collect::<Vec<_>>(),
        );

        let aac = Some(AudioTranscode {
            codec: AudioTranscodeCodec::Aac,
            channels: 2,
        });
        assert_eq!(
            (decision.method, decision.reasons, decision.audio_transcodes),
            (PlayMethod::Transcode, reasons, vec![aac, None, aac])
        );
        assert_eq!(decision.video_transcode, None);
    }
}
