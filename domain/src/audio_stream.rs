use crate::MediaFile;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct AudioStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub channels: i64,
    pub title: Option<String>,
    pub is_default: bool,
}

impl AudioStream {
    pub fn selected_by_default(streams: &[Self]) -> Option<&Self> {
        streams.iter().min_by_key(|x| (!x.is_default, x.index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toasty::Deferred;

    fn stream(index: i64, is_default: bool) -> AudioStream {
        AudioStream {
            id: Uuid::from_u128(index as u128),
            media_file_id: Uuid::nil(),
            media_file: Deferred::default(),
            index,
            codec: "aac".to_owned(),
            language: None,
            channels: 2,
            title: None,
            is_default,
        }
    }

    #[test]
    fn the_default_audio_is_the_files_default_track() {
        let streams = [stream(1, false), stream(3, true), stream(2, true)];

        assert_eq!(
            AudioStream::selected_by_default(&streams).map(|x| x.index),
            Some(2)
        );
    }

    #[test]
    fn without_a_default_track_the_default_audio_is_the_first_track() {
        let streams = [stream(3, false), stream(1, false)];

        assert_eq!(
            AudioStream::selected_by_default(&streams).map(|x| x.index),
            Some(1)
        );
        assert!(AudioStream::selected_by_default(&[]).is_none());
    }
}
