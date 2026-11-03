use crate::AudioStream;
use crate::Episode;
use crate::Movie;
use crate::SubtitleFile;
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
