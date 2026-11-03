use crate::MediaFolder;
use crate::SourceHealth;
use crate::ports::GetSonarrShow;
use crate::ports::ReadSonarrWebhook;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub get_sonarr_show: GetSonarrShow,
    pub media_folder: MediaFolder,
    pub read_sonarr_webhook: ReadSonarrWebhook,
    pub source_health: SourceHealth,
}
