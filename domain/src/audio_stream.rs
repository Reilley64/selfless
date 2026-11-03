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
