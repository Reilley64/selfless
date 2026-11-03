use crate::AudioCodecSupport;
use crate::AudioStream;
use crate::AudioTranscode;
use crate::AudioTranscodeCodec;
use crate::ClientKind;
use crate::MediaFile;
use crate::PlayReason;
use crate::SubtitleFile;
use crate::SubtitleStream;
use crate::SubtitleTrack;
use crate::TrackSelection;
use crate::VideoCodecSupport;
use crate::VideoStream;

const MAX_TRANSCODE_CHANNELS: i64 = 6;

#[derive(Clone, Debug, Default)]
pub struct CapabilityProfile {
    pub containers: Vec<String>,
    pub video_codecs: Vec<VideoCodecSupport>,
    pub audio_codecs: Vec<AudioCodecSupport>,
    pub subtitle_codecs: Vec<String>,
}

impl CapabilityProfile {
    fn direct_play_reasons(
        &self,
        container: &str,
        video: Option<&VideoStream>,
        audio: Option<&AudioStream>,
        subtitle: Option<SubtitleTrack>,
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
        reasons.extend(audio.and_then(|x| self.audio_reason(x)));
        match subtitle {
            Some(SubtitleTrack::Stream(x)) if self.subtitle_codecs.contains(&x.codec) => {}
            Some(x) => reasons.push(PlayReason::Subtitle(x.codec())),
            None => {}
        }
        reasons
    }

    pub fn play_reasons(
        &self,
        client_kind: ClientKind,
        media_file: &MediaFile,
        container: &str,
        tracks: TrackSelection,
        force_hls: bool,
    ) -> Vec<PlayReason> {
        let mut reasons = self.direct_play_reasons(
            container,
            media_file.video_stream(),
            tracks.audio,
            tracks.subtitle,
        );
        for reason in self.tv_subtitle_reasons(
            client_kind,
            media_file.subtitle_streams.get(),
            media_file.subtitle_files.get(),
        ) {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        if force_hls && reasons.is_empty() {
            reasons.push(PlayReason::ForcedHls);
        }
        reasons
    }

    pub fn shows_subtitle(&self, subtitle: SubtitleTrack) -> bool {
        subtitle.text() || self.subtitle_codecs.contains(&subtitle.codec())
    }

    fn tv_subtitle_reasons(
        &self,
        client_kind: ClientKind,
        streams: &[SubtitleStream],
        files: &[SubtitleFile],
    ) -> Vec<PlayReason> {
        if client_kind == ClientKind::Web {
            return Vec::new();
        }
        let mut reasons = Vec::new();
        let uncovered = streams
            .iter()
            .filter(|x| x.text && !self.subtitle_codecs.contains(&x.codec))
            .map(|x| x.codec.clone())
            .chain(files.iter().map(SubtitleFile::codec));
        for codec in uncovered {
            let reason = PlayReason::Subtitle(codec);
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        reasons
    }

    pub fn audio_reason(&self, audio: &AudioStream) -> Option<PlayReason> {
        match self.audio_codec(&audio.codec) {
            None => Some(PlayReason::AudioCodec(audio.codec.clone())),
            Some(support) if audio.channels > support.max_channels => {
                Some(PlayReason::AudioChannels(audio.channels))
            }
            Some(_) => None,
        }
    }

    pub fn audio_transcode(&self, audio: &AudioStream) -> AudioTranscode {
        let (codec, support) = match self.audio_codec("eac3") {
            Some(support) => (AudioTranscodeCodec::Eac3, Some(support)),
            None => (AudioTranscodeCodec::Aac, self.audio_codec("aac")),
        };
        AudioTranscode {
            codec,
            channels: audio
                .channels
                .min(support.map_or(MAX_TRANSCODE_CHANNELS, |x| x.max_channels))
                .min(MAX_TRANSCODE_CHANNELS),
        }
    }

    fn audio_codec(&self, codec: &str) -> Option<&AudioCodecSupport> {
        self.audio_codecs.iter().find(|x| x.codec == codec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HdrType;
    use crate::SubtitleFile;
    use crate::SubtitleStream;
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
            interlaced: false,
            bit_rate: None,
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

    fn subtitle_stream(codec: &str, text: bool) -> SubtitleStream {
        SubtitleStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 2,
            codec: codec.to_owned(),
            language: Some("eng".to_owned()),
            title: None,
            is_default: false,
            forced: false,
            hearing_impaired: false,
            text,
        }
    }

    fn subtitle_file(path: &str) -> SubtitleFile {
        SubtitleFile {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            path: path.to_owned(),
            language: Some("en".to_owned()),
            hearing_impaired: false,
            forced: false,
        }
    }

    fn media_file(streams: Vec<SubtitleStream>, files: Vec<SubtitleFile>) -> MediaFile {
        MediaFile {
            id: Uuid::nil(),
            path: "/movies/Arrival.mp4".to_owned(),
            size: None,
            modified_at: None,
            container: None,
            duration_ms: None,
            movies: Deferred::default(),
            episodes: Deferred::default(),
            video_streams: Deferred::from(vec![video("h264", "High", 1920, 800, HdrType::Sdr)]),
            audio_streams: Deferred::default(),
            subtitle_streams: Deferred::from(streams),
            subtitle_files: Deferred::from(files),
            sync_problems: Deferred::default(),
            keyframe_index: Deferred::default(),
        }
    }

    #[test]
    fn the_play_reasons_hold_each_direct_play_and_tv_subtitle_reason_once() {
        let file = media_file(Vec::new(), vec![subtitle_file("/movies/Arrival.en.srt")]);
        let subtitle = SubtitleTrack::File(&file.subtitle_files.get()[0]);

        let reasons = browser().play_reasons(
            ClientKind::GoogleTv,
            &file,
            "mkv",
            TrackSelection {
                audio: None,
                subtitle: Some(subtitle),
            },
            false,
        );

        assert_eq!(
            reasons,
            [
                PlayReason::Container("mkv".to_owned()),
                PlayReason::Subtitle("srt".to_owned()),
            ]
        );
    }

    #[test]
    fn forcing_hls_is_a_reason_only_for_a_file_that_would_play_directly() {
        let file = media_file(Vec::new(), Vec::new());
        let reasons = |container| {
            browser().play_reasons(
                ClientKind::Web,
                &file,
                container,
                TrackSelection::default(),
                true,
            )
        };

        assert_eq!(reasons("mp4"), [PlayReason::ForcedHls]);
        assert_eq!(reasons("mkv"), [PlayReason::Container("mkv".to_owned())]);
    }

    #[test]
    fn on_a_tv_every_text_subtitle_must_be_embedded_in_a_codec_the_profile_lists() {
        let mut profile = browser();
        profile.subtitle_codecs = vec!["subrip".to_owned()];
        let covered = [
            subtitle_stream("subrip", true),
            subtitle_stream("hdmv_pgs_subtitle", false),
        ];
        let uncovered = [
            subtitle_stream("subrip", true),
            subtitle_stream("ass", true),
            subtitle_stream("ass", true),
        ];

        let reasons = [
            profile.tv_subtitle_reasons(ClientKind::GoogleTv, &covered, &[]),
            profile.tv_subtitle_reasons(
                ClientKind::AppleTv,
                &uncovered,
                &[subtitle_file("/movies/Arrival.en.srt")],
            ),
        ];

        assert_eq!(
            reasons,
            [
                vec![],
                vec![
                    PlayReason::Subtitle("ass".to_owned()),
                    PlayReason::Subtitle("srt".to_owned()),
                ],
            ]
        );
    }

    #[test]
    fn a_browser_plays_directly_whatever_text_subtitles_the_file_has() {
        let reasons = browser().tv_subtitle_reasons(
            ClientKind::Web,
            &[subtitle_stream("ass", true)],
            &[subtitle_file("/movies/Arrival.en.srt")],
        );

        assert_eq!(reasons, []);
    }

    #[test]
    fn a_profile_that_covers_the_container_video_and_audio_plays_the_file_directly() {
        let reasons = browser().direct_play_reasons(
            "mp4",
            Some(&video("h264", "High", 1920, 800, HdrType::Sdr)),
            Some(&audio("aac", 2)),
            None,
        );

        assert_eq!(reasons, []);
    }

    #[test]
    fn each_part_the_profile_does_not_cover_is_a_reason() {
        let reasons = browser().direct_play_reasons(
            "mkv",
            Some(&video("h264", "High 10", 3840, 1600, HdrType::Hdr10)),
            Some(&audio("aac", 6)),
            None,
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
            None,
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
    fn a_direct_play_needs_the_selected_subtitle_embedded_in_a_codec_the_profile_lists() {
        let mut profile = browser();
        profile.subtitle_codecs = vec!["hdmv_pgs_subtitle".to_owned()];
        let streams = ["hdmv_pgs_subtitle", "subrip"].map(|codec| SubtitleStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 2,
            codec: codec.to_owned(),
            language: None,
            title: None,
            is_default: false,
            forced: false,
            hearing_impaired: false,
            text: codec == "subrip",
        });
        let file = SubtitleFile {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            path: "/movies/Arrival.en.srt".to_owned(),
            language: None,
            hearing_impaired: false,
            forced: false,
        };

        let reasons = [
            SubtitleTrack::Stream(&streams[0]),
            SubtitleTrack::Stream(&streams[1]),
            SubtitleTrack::File(&file),
        ]
        .map(|x| profile.direct_play_reasons("mp4", None, None, Some(x)));

        assert_eq!(
            reasons,
            [
                vec![],
                vec![PlayReason::Subtitle("subrip".to_owned())],
                vec![PlayReason::Subtitle("srt".to_owned())],
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
            None,
        );

        assert_eq!(reasons, []);
    }

    #[test]
    fn a_profile_with_e_ac_3_transcodes_audio_to_e_ac_3_with_the_sources_channels_up_to_5_1() {
        let mut profile = browser();
        profile.audio_codecs.push(AudioCodecSupport {
            codec: "eac3".to_owned(),
            max_channels: 8,
        });

        let targets = [8, 6, 2].map(|x| profile.audio_transcode(&audio("truehd", x)));

        assert_eq!(
            targets,
            [6, 6, 2].map(|channels| AudioTranscode {
                codec: AudioTranscodeCodec::Eac3,
                channels,
            })
        );
    }

    #[test]
    fn a_profile_without_e_ac_3_transcodes_audio_to_aac_with_the_sources_channels_up_to_its_maximum()
     {
        let mut surround = browser();
        surround.audio_codecs[0].max_channels = 8;

        let targets = [
            browser().audio_transcode(&audio("eac3", 6)),
            browser().audio_transcode(&audio("dts", 1)),
            surround.audio_transcode(&audio("dts", 8)),
        ];

        assert_eq!(
            targets,
            [2, 1, 6].map(|channels| AudioTranscode {
                codec: AudioTranscodeCodec::Aac,
                channels,
            })
        );
    }
}
