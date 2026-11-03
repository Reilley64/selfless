use crate::MediaFolder;
use crate::SourceHealth;
use crate::ports::GetRadarrMovie;
use crate::ports::ReadRadarrWebhook;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_radarr_movie: GetRadarrMovie,
    pub media_folder: MediaFolder,
    pub read_radarr_webhook: ReadRadarrWebhook,
    pub source_health: SourceHealth,
}
