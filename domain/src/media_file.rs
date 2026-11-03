use crate::AudioStream;
use crate::Episode;
use crate::KeyframeIndex;
use crate::Movie;
use crate::SubtitleFile;
use crate::SubtitleLanguage;
use crate::SubtitleStream;
use crate::SyncProblem;
use crate::VideoStream;
use crate::needs_reread;
use jiff::Timestamp;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct MediaFile {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub path: String,
    pub size: Option<i64>,
    pub modified_at: Option<Timestamp>,
    pub container: Option<String>,
    pub duration_ms: Option<i64>,
    #[has_many]
    pub movies: toasty::Deferred<Vec<Movie>>,
    #[has_many]
    pub episodes: toasty::Deferred<Vec<Episode>>,
    #[has_many]
    pub video_streams: toasty::Deferred<Vec<VideoStream>>,
    #[has_many]
    pub audio_streams: toasty::Deferred<Vec<AudioStream>>,
    #[has_many]
    pub subtitle_streams: toasty::Deferred<Vec<SubtitleStream>>,
    #[has_many]
    pub subtitle_files: toasty::Deferred<Vec<SubtitleFile>>,
    #[has_many]
    pub sync_problems: toasty::Deferred<Vec<SyncProblem>>,
    #[has_one]
    pub keyframe_index: toasty::Deferred<Option<KeyframeIndex>>,
}

impl MediaFile {
    pub fn needs_probe(&self, size: i64, modified_at: Timestamp) -> bool {
        self.size != Some(size) || needs_reread(self.modified_at, modified_at)
    }

    pub fn runtime_minutes(&self) -> Option<i64> {
        self.duration_ms.map(|x| (x + 30_000) / 60_000)
    }

    pub fn container_name(&self) -> Option<&str> {
        let extension = self
            .path
            .rsplit_once('.')
            .map(|(_, x)| x.to_lowercase())
            .unwrap_or_default();
        Some(match self.container.as_deref()? {
            "matroska,webm" if extension == "webm" => "webm",
            "matroska,webm" => "mkv",
            "mov,mp4,m4a,3gp,3g2,mj2" if extension == "mov" => "mov",
            "mov,mp4,m4a,3gp,3g2,mj2" => "mp4",
            container => container.split(',').next().unwrap_or(container),
        })
    }

    pub fn audio_languages(&self) -> Vec<String> {
        let mut streams: Vec<&AudioStream> = self.audio_streams.get().iter().collect();
        streams.sort_by_key(|x| x.index);
        let mut languages = Vec::new();
        for language in streams.into_iter().filter_map(|x| x.language.clone()) {
            if !languages.contains(&language) {
                languages.push(language);
            }
        }
        languages
    }

    pub fn subtitle_languages(&self) -> Vec<SubtitleLanguage> {
        let mut streams: Vec<&SubtitleStream> = self
            .subtitle_streams
            .get()
            .iter()
            .filter(|x| x.text)
            .collect();
        streams.sort_by_key(|x| x.index);
        let mut files: Vec<&SubtitleFile> = self.subtitle_files.get().iter().collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let subtitles = streams
            .into_iter()
            .map(|x| (&x.language, x.hearing_impaired, x.forced))
            .chain(
                files
                    .into_iter()
                    .map(|x| (&x.language, x.hearing_impaired, x.forced)),
            )
            .filter_map(|(language, hearing_impaired, forced)| {
                Some(SubtitleLanguage {
                    language: language.clone()?,
                    hearing_impaired,
                    forced,
                })
            });
        let mut languages = Vec::new();
        for subtitle in subtitles {
            if !languages.contains(&subtitle) {
                languages.push(subtitle);
            }
        }
        languages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    const MODIFIED_AT: &str = "2026-10-29T10:00:00Z";

    fn at(timestamp: &str) -> Timestamp {
        timestamp.parse().expect("the timestamp is valid")
    }

    fn media_file(size: Option<i64>, modified_at: Option<&str>) -> MediaFile {
        MediaFile {
            id: Uuid::nil(),
            path: "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv".to_owned(),
            size,
            modified_at: modified_at.map(at),
            container: None,
            duration_ms: None,
            movies: Deferred::default(),
            episodes: Deferred::default(),
            video_streams: Deferred::default(),
            audio_streams: Deferred::default(),
            subtitle_streams: Deferred::default(),
            subtitle_files: Deferred::default(),
            sync_problems: Deferred::default(),
            keyframe_index: Deferred::default(),
        }
    }

    #[test]
    fn the_runtime_is_the_probed_duration_rounded_to_the_nearest_minute() {
        let runtime = |duration_ms| {
            MediaFile {
                duration_ms,
                ..media_file(None, None)
            }
            .runtime_minutes()
        };

        assert_eq!(runtime(Some(2_829_999)), Some(47));
        assert_eq!(runtime(Some(2_850_000)), Some(48));
        assert_eq!(runtime(None), None);
    }

    fn audio(index: i64, language: Option<&str>) -> AudioStream {
        AudioStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index,
            codec: "eac3".to_owned(),
            language: language.map(str::to_owned),
            channels: 6,
            title: None,
            is_default: false,
        }
    }

    fn subtitle_stream(
        index: i64,
        language: &str,
        (text, hearing_impaired, forced): (bool, bool, bool),
    ) -> SubtitleStream {
        SubtitleStream {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index,
            codec: if text { "subrip" } else { "hdmv_pgs_subtitle" }.to_owned(),
            language: Some(language.to_owned()),
            title: None,
            is_default: false,
            forced,
            hearing_impaired,
            text,
        }
    }

    fn subtitle_file(name: &str, language: &str, hearing_impaired: bool) -> SubtitleFile {
        SubtitleFile {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            path: format!("/media/TV/Severance/Season 01/{name}"),
            language: Some(language.to_owned()),
            hearing_impaired,
            forced: false,
        }
    }

    fn with_tracks(
        audio_streams: Vec<AudioStream>,
        subtitle_streams: Vec<SubtitleStream>,
        subtitle_files: Vec<SubtitleFile>,
    ) -> MediaFile {
        MediaFile {
            audio_streams: Deferred::from(audio_streams),
            subtitle_streams: Deferred::from(subtitle_streams),
            subtitle_files: Deferred::from(subtitle_files),
            ..media_file(None, None)
        }
    }

    fn subtitle(language: &str, hearing_impaired: bool, forced: bool) -> SubtitleLanguage {
        SubtitleLanguage {
            language: language.to_owned(),
            hearing_impaired,
            forced,
        }
    }

    #[test]
    fn the_audio_languages_follow_the_stream_order_once_each() {
        let file = with_tracks(
            vec![
                audio(2, Some("jpn")),
                audio(1, Some("eng")),
                audio(3, Some("eng")),
                audio(4, None),
            ],
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(file.audio_languages(), ["eng", "jpn"]);
    }

    #[test]
    fn the_subtitle_languages_are_the_text_subtitles_in_the_file_then_the_subtitle_files_beside_it()
    {
        let file = with_tracks(
            Vec::new(),
            vec![
                subtitle_stream(6, "eng", (true, true, false)),
                subtitle_stream(5, "eng", (true, false, true)),
                subtitle_stream(7, "eng", (true, false, true)),
            ],
            vec![
                subtitle_file("Severance - S01E02-E03.fr.hi.srt", "fr", true),
                subtitle_file("Severance - S01E02-E03.en.srt", "en", false),
            ],
        );

        assert_eq!(
            file.subtitle_languages(),
            [
                subtitle("eng", false, true),
                subtitle("eng", true, false),
                subtitle("en", false, false),
                subtitle("fr", true, false),
            ]
        );
    }

    #[test]
    fn image_subtitles_are_left_out_of_the_subtitle_languages() {
        let file = with_tracks(
            Vec::new(),
            vec![
                subtitle_stream(2, "eng", (false, false, false)),
                subtitle_stream(3, "deu", (false, false, true)),
                subtitle_stream(4, "spa", (true, false, false)),
            ],
            Vec::new(),
        );

        assert_eq!(file.subtitle_languages(), [subtitle("spa", false, false)]);
    }

    #[test]
    fn the_container_name_tells_matroska_from_webm_and_mp4_from_mov_by_extension() {
        let name = |path: &str, container: Option<&str>| {
            MediaFile {
                path: path.to_owned(),
                container: container.map(str::to_owned),
                ..media_file(None, None)
            }
            .container_name()
            .map(str::to_owned)
        };
        let matroska = Some("matroska,webm");
        let mp4 = Some("mov,mp4,m4a,3gp,3g2,mj2");

        assert_eq!(name("/m/a.mkv", matroska).as_deref(), Some("mkv"));
        assert_eq!(name("/m/a.WebM", matroska).as_deref(), Some("webm"));
        assert_eq!(name("/m/a.mp4", mp4).as_deref(), Some("mp4"));
        assert_eq!(name("/m/a.m4v", mp4).as_deref(), Some("mp4"));
        assert_eq!(name("/m/a.mov", mp4).as_deref(), Some("mov"));
        assert_eq!(name("/m/a.ts", Some("mpegts")).as_deref(), Some("mpegts"));
        assert_eq!(name("/m/a.mkv", None), None);
    }

    #[test]
    fn a_file_with_the_stored_size_and_modification_time_needs_no_probe() {
        assert!(!media_file(Some(100), Some(MODIFIED_AT)).needs_probe(100, at(MODIFIED_AT)));
    }

    #[test]
    fn a_file_whose_size_changed_needs_a_probe() {
        assert!(media_file(Some(100), Some(MODIFIED_AT)).needs_probe(101, at(MODIFIED_AT)));
    }

    #[test]
    fn a_file_whose_modification_time_changed_needs_a_probe() {
        assert!(
            media_file(Some(100), Some(MODIFIED_AT)).needs_probe(100, at("2026-10-30T10:00:00Z"))
        );
    }

    #[test]
    fn a_file_that_was_missing_on_disk_needs_a_probe() {
        assert!(media_file(None, None).needs_probe(100, at(MODIFIED_AT)));
    }
}
