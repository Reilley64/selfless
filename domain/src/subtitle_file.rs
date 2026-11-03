use crate::MediaFile;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct SubtitleFile {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<MediaFile>,
    pub path: String,
    pub language: Option<String>,
    pub hearing_impaired: bool,
    pub forced: bool,
}

impl SubtitleFile {
    pub fn codec(&self) -> String {
        self.path
            .rsplit_once('.')
            .map(|(_, x)| x.to_lowercase())
            .unwrap_or_default()
    }
}
