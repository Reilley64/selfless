use crate::MediaFile;
use jiff::Timestamp;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct KeyframeIndex {
    #[key]
    #[auto]
    pub id: Uuid,
    #[unique]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<MediaFile>,
    pub size: Option<i64>,
    pub modified_at: Option<Timestamp>,
    pub keyframes_ns: Vec<i64>,
}

impl KeyframeIndex {
    pub fn nanoseconds(keyframes: &[Duration]) -> Vec<i64> {
        keyframes
            .iter()
            .map(|x| i64::try_from(x.as_nanos()).unwrap_or(i64::MAX))
            .collect()
    }

    pub fn is_current_for(&self, media_file: &MediaFile) -> bool {
        self.size == media_file.size && self.modified_at == media_file.modified_at
    }

    pub fn keyframes(&self) -> Vec<Duration> {
        self.keyframes_ns
            .iter()
            .map(|x| Duration::from_nanos(u64::try_from(*x).unwrap_or_default()))
            .collect()
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

    fn media_file(size: i64, modified_at: &str) -> MediaFile {
        MediaFile {
            id: Uuid::nil(),
            path: "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv".to_owned(),
            size: Some(size),
            modified_at: Some(at(modified_at)),
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

    fn index(keyframes: &[Duration]) -> KeyframeIndex {
        KeyframeIndex {
            id: Uuid::nil(),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            size: Some(100),
            modified_at: Some(at(MODIFIED_AT)),
            keyframes_ns: KeyframeIndex::nanoseconds(keyframes),
        }
    }

    #[test]
    fn an_index_is_current_only_for_the_size_and_modification_time_it_was_read_at() {
        let index = index(&[]);

        assert!(index.is_current_for(&media_file(100, MODIFIED_AT)));
        assert!(!index.is_current_for(&media_file(101, MODIFIED_AT)));
        assert!(!index.is_current_for(&media_file(100, "2026-10-30T10:00:00Z")));
    }

    #[test]
    fn an_index_keeps_its_keyframe_times_to_the_nanosecond() {
        let keyframes = [Duration::ZERO, Duration::from_nanos(6_006_000_001)];

        assert_eq!(index(&keyframes).keyframes(), keyframes);
    }
}
