use crate::AudioStream;
use crate::CapabilityProfile;
use crate::PlayMethod;
use crate::SubtitleFile;
use crate::SubtitleStream;
use crate::TrackLanguage;
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub enum SubtitleTrack<'a> {
    Stream(&'a SubtitleStream),
    File(&'a SubtitleFile),
}

impl<'a> SubtitleTrack<'a> {
    pub fn all(streams: &'a [SubtitleStream], files: &'a [SubtitleFile]) -> Vec<Self> {
        let mut streams = streams.iter().collect::<Vec<_>>();
        streams.sort_by_key(|x| x.index);
        let mut files = files.iter().collect::<Vec<_>>();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        streams
            .into_iter()
            .map(Self::Stream)
            .chain(files.into_iter().map(Self::File))
            .collect()
    }

    pub fn selected_by_default(
        tracks: &[Self],
        profile: &CapabilityProfile,
        subtitle_language: Option<&str>,
        audio: Option<&AudioStream>,
    ) -> Option<Self> {
        let mut shown = tracks
            .iter()
            .copied()
            .filter(|x| profile.shows_subtitle(*x));
        match subtitle_language {
            Some(language) => shown
                .filter(|x| TrackLanguage::matches(x.language(), language))
                .min_by_key(|x| (x.forced(), x.hearing_impaired())),
            None => {
                let audio_language = audio.and_then(|x| x.language.as_deref())?;
                shown.find(|x| {
                    x.forced()
                        && TrackLanguage::matches(
                            x.language(),
                            &TrackLanguage::remembered(audio_language),
                        )
                })
            }
        }
    }

    pub fn id(&self) -> Uuid {
        match self {
            Self::Stream(x) => x.id,
            Self::File(x) => x.id,
        }
    }

    pub fn codec(&self) -> String {
        match self {
            Self::Stream(x) => x.codec.clone(),
            Self::File(x) => x.codec(),
        }
    }

    pub fn language(&self) -> Option<&'a str> {
        match self {
            Self::Stream(x) => x.language.as_deref(),
            Self::File(x) => x.language.as_deref(),
        }
    }

    pub fn in_stream(&self, method: PlayMethod, profile: &CapabilityProfile) -> bool {
        match (method, self) {
            (PlayMethod::DirectPlay, Self::Stream(x)) => profile.subtitle_codecs.contains(&x.codec),
            (PlayMethod::DirectPlay, Self::File(_)) => false,
            (PlayMethod::Remux | PlayMethod::Transcode, x) => x.text(),
        }
    }

    pub fn text(&self) -> bool {
        match self {
            Self::Stream(x) => x.text,
            Self::File(_) => true,
        }
    }

    fn forced(&self) -> bool {
        match self {
            Self::Stream(x) => x.forced,
            Self::File(x) => x.forced,
        }
    }

    fn hearing_impaired(&self) -> bool {
        match self {
            Self::Stream(x) => x.hearing_impaired,
            Self::File(x) => x.hearing_impaired,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    struct Subtitle {
        language: &'static str,
        forced: bool,
        hearing_impaired: bool,
        codec: &'static str,
    }

    const REGULAR: Subtitle = Subtitle {
        language: "eng",
        forced: false,
        hearing_impaired: false,
        codec: "subrip",
    };

    fn streams(subtitles: &[Subtitle]) -> Vec<SubtitleStream> {
        subtitles
            .iter()
            .zip(1..)
            .map(|(x, index)| SubtitleStream {
                id: Uuid::from_u128(index as u128),
                media_file_id: Uuid::nil(),
                media_file: Deferred::default(),
                index,
                codec: x.codec.to_owned(),
                language: Some(x.language.to_owned()),
                title: None,
                is_default: false,
                forced: x.forced,
                hearing_impaired: x.hearing_impaired,
                text: x.codec != "hdmv_pgs_subtitle",
            })
            .collect()
    }

    fn file(language: &str, forced: bool) -> SubtitleFile {
        SubtitleFile {
            id: Uuid::from_u128(100),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            path: format!("/movies/Arrival.{language}.srt"),
            language: Some(language.to_owned()),
            hearing_impaired: false,
            forced,
        }
    }

    fn audio(language: &str) -> AudioStream {
        AudioStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index: 0,
            codec: "aac".to_owned(),
            language: Some(language.to_owned()),
            channels: 2,
            title: None,
            is_default: true,
        }
    }

    fn selected(
        streams: &[SubtitleStream],
        files: &[SubtitleFile],
        profile: &CapabilityProfile,
        subtitle_language: Option<&str>,
        audio_language: &str,
    ) -> Option<Uuid> {
        SubtitleTrack::selected_by_default(
            &SubtitleTrack::all(streams, files),
            profile,
            subtitle_language,
            Some(&audio(audio_language)),
        )
        .map(|x| x.id())
    }

    #[test]
    fn the_default_subtitle_is_a_track_in_the_remembered_language_preferring_one_neither_forced_nor_hearing_impaired()
     {
        let streams = streams(&[
            Subtitle {
                language: "fre",
                ..REGULAR
            },
            Subtitle {
                forced: true,
                ..REGULAR
            },
            Subtitle {
                hearing_impaired: true,
                ..REGULAR
            },
            REGULAR,
            REGULAR,
        ]);

        let id = selected(
            &streams,
            &[],
            &CapabilityProfile::default(),
            Some("eng"),
            "eng",
        );

        assert_eq!(id, Some(streams[3].id));
    }

    #[test]
    fn a_forced_or_hearing_impaired_track_is_the_default_subtitle_when_the_remembered_language_has_no_other()
     {
        let streams = streams(&[
            Subtitle {
                forced: true,
                ..REGULAR
            },
            Subtitle {
                hearing_impaired: true,
                ..REGULAR
            },
        ]);

        let id = selected(
            &streams,
            &[],
            &CapabilityProfile::default(),
            Some("eng"),
            "eng",
        );

        assert_eq!(id, Some(streams[1].id));
    }

    #[test]
    fn a_subtitle_file_in_the_remembered_language_can_be_the_default_subtitle() {
        let files = [file("fr", false)];

        let id = selected(
            &[],
            &files,
            &CapabilityProfile::default(),
            Some("fra"),
            "eng",
        );

        assert_eq!(id, Some(files[0].id));
    }

    #[test]
    fn an_image_subtitle_is_the_default_subtitle_only_where_the_profile_shows_it() {
        let streams = streams(&[Subtitle {
            codec: "hdmv_pgs_subtitle",
            ..REGULAR
        }]);
        let google_tv = CapabilityProfile {
            subtitle_codecs: vec!["hdmv_pgs_subtitle".to_owned()],
            ..CapabilityProfile::default()
        };

        let ids = [CapabilityProfile::default(), google_tv]
            .map(|profile| selected(&streams, &[], &profile, Some("eng"), "eng"));

        assert_eq!(ids, [None, Some(streams[0].id)]);
    }

    #[test]
    fn with_subtitles_off_a_forced_subtitle_in_the_audios_language_turns_on() {
        let streams = streams(&[
            REGULAR,
            Subtitle {
                language: "fre",
                forced: true,
                ..REGULAR
            },
        ]);
        let files = [file("en", true)];

        let english = selected(&streams, &files, &CapabilityProfile::default(), None, "eng");
        let japanese = selected(&streams, &files, &CapabilityProfile::default(), None, "jpn");

        assert_eq!(english, Some(files[0].id));
        assert_eq!(japanese, None);
    }

    #[test]
    fn a_direct_play_carries_only_embedded_subtitles_the_profile_lists_and_hls_carries_every_text_subtitle()
     {
        let streams = streams(&[
            REGULAR,
            Subtitle {
                codec: "hdmv_pgs_subtitle",
                ..REGULAR
            },
        ]);
        let files = [file("en", false)];
        let tracks = SubtitleTrack::all(&streams, &files);
        let google_tv = CapabilityProfile {
            subtitle_codecs: vec!["hdmv_pgs_subtitle".to_owned()],
            ..CapabilityProfile::default()
        };

        let in_stream = [
            PlayMethod::DirectPlay,
            PlayMethod::Remux,
            PlayMethod::Transcode,
        ]
        .map(|method| {
            tracks
                .iter()
                .map(|x| x.in_stream(method, &google_tv))
                .collect::<Vec<_>>()
        });

        assert_eq!(
            in_stream,
            [
                vec![false, true, false],
                vec![true, false, true],
                vec![true, false, true],
            ]
        );
    }
}
