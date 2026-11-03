use crate::library::Prober;
use crate::library::SourceHealth;
use crate::ports::FindMediaFile;
use crate::ports::GetSonarrShow;
use crate::ports::ListSubtitleFiles;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ReadSonarrWebhook;
use crate::ports::ReadWebhookEventType;
use crate::ports::persistence::DbContext;

#[fundle::deps]
pub struct Dependencies {
    pub db_context: DbContext,
    pub find_media_file: FindMediaFile,
    pub get_sonarr_show: GetSonarrShow,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
    pub read_sonarr_webhook: ReadSonarrWebhook,
    pub read_webhook_event_type: ReadWebhookEventType,
    pub source_health: SourceHealth,
}
