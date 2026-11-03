use crate::HdrType;
use crate::MediaFile;
use uuid::Uuid;

#[derive(Debug, toasty::Model)]
pub struct VideoStream {
    #[key]
    #[auto]
    pub id: Uuid,
    #[index]
    pub media_file_id: Uuid,
    #[belongs_to(key = media_file_id, references = id)]
    pub media_file: toasty::Deferred<MediaFile>,
    pub index: i64,
    pub codec: String,
    pub profile: Option<String>,
    pub width: i64,
    pub height: i64,
    pub frame_rate: Option<f64>,
    pub hdr_type: HdrType,
    pub interlaced: bool,
    pub bit_rate: Option<i64>,
}
