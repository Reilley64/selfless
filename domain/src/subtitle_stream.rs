use crate::MediaFile;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct SubtitleStream {
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
    pub title: Option<String>,
    pub is_default: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    pub text: bool,
}
