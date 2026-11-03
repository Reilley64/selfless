mod passwords;
mod tokens;

pub mod actuator;
pub mod admins;
pub mod invites;
pub mod library;
pub mod ports;
pub mod sessions;
pub mod telemetry;
pub mod trace_context;
pub mod users;

pub mod confirm_held_removal;
pub mod get_media_file;
mod library_items;
pub mod list_episodes;
pub mod list_movies;
pub mod list_seasons;
pub mod list_shows;
mod media_files;
mod metadata;
pub mod reconcile;
mod source_states;
pub mod store_webhook_delivery;
pub mod sync_movie;
pub mod sync_now;
pub mod sync_show;
pub mod sync_status;
mod webhook_deliveries;

#[cfg(test)]
#[path = "../../test/application.rs"]
mod test;

pub mod attributes {
    include!(concat!(env!("OUT_DIR"), "/attributes.rs"));
}

pub mod metrics {
    include!(concat!(env!("OUT_DIR"), "/metrics.rs"));
}

pub mod spans {
    include!(concat!(env!("OUT_DIR"), "/spans.rs"));
}

use domain::Source;
use ports::FindMediaFile;
use ports::ListSubtitleFiles;
use ports::Probe;
use ports::ProbeError;
use ports::ProbeMediaFile;
use ports::ReadEpisodeFolder;
use ports::ReadEpisodeNfo;
use ports::ReadMovieFolder;
use ports::ReadMovieNfo;
use ports::ReadShowFolder;
use ports::ReadShowNfo;
use std::sync::Arc;
use std::sync::Mutex;
use sync_status::ApiCall;
use sync_status::ReconcileProgress;
use tokio::sync::Notify;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit_id: &'static str,
    pub git_commit_time: &'static str,
}

#[derive(Clone, Default)]
pub struct BySource<T> {
    pub radarr: T,
    pub sonarr: T,
}

impl<T> BySource<T> {
    pub fn get(&self, source: Source) -> &T {
        match source {
            Source::Radarr => &self.radarr,
            Source::Sonarr => &self.sonarr,
        }
    }
}

pub type WebhookPasswords = BySource<String>;

#[derive(Clone, Default)]
pub struct Reconciles {
    requested: Arc<Notify>,
    sources: BySource<SourceReconcile>,
}

#[derive(Clone, Default)]
struct SourceReconcile {
    running: Arc<tokio::sync::Mutex<()>>,
    progress: Arc<Mutex<Option<ReconcileProgress>>>,
}

impl Reconciles {
    pub async fn requested(&self) {
        self.requested.notified().await;
    }

    fn request(&self) {
        self.requested.notify_one();
    }

    fn source(&self, source: Source) -> &SourceReconcile {
        self.sources.get(source)
    }

    fn progress(&self, source: Source) -> Option<ReconcileProgress> {
        *self.source(source).progress.lock().unwrap()
    }
}

#[derive(Clone, Default)]
pub struct SourceHealth(BySource<Arc<Mutex<Health>>>);

#[derive(Clone, Default)]
struct Health {
    last_api_call: Option<ApiCall>,
    kodi_metadata: Option<bool>,
}

impl SourceHealth {
    fn api_call<T, E: std::error::Error>(
        &self,
        source: Source,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.0.get(source).lock().unwrap().last_api_call = Some(match &result {
            Ok(_) => ApiCall::Succeeded,
            Err(error) => ApiCall::Failed(error.to_string()),
        });
        result
    }

    fn set_kodi_metadata(&self, source: Source, enabled: bool) {
        self.0.get(source).lock().unwrap().kodi_metadata = Some(enabled);
    }

    fn health(&self, source: Source) -> Health {
        self.0.get(source).lock().unwrap().clone()
    }
}

pub(crate) const CONCURRENT_PROBES: usize = 4;

#[derive(Clone)]
pub struct MediaFolder {
    pub find_media_file: FindMediaFile,
    pub list_subtitle_files: ListSubtitleFiles,
    pub prober: Prober,
    pub read_episode_folder: ReadEpisodeFolder,
    pub read_episode_nfo: ReadEpisodeNfo,
    pub read_movie_folder: ReadMovieFolder,
    pub read_movie_nfo: ReadMovieNfo,
    pub read_show_folder: ReadShowFolder,
    pub read_show_nfo: ReadShowNfo,
}

#[derive(Clone)]
pub struct Prober {
    probe_media_file: ProbeMediaFile,
    slots: Arc<Semaphore>,
}

impl Prober {
    pub fn new(probe_media_file: ProbeMediaFile) -> Self {
        Self {
            probe_media_file,
            slots: Arc::new(Semaphore::new(CONCURRENT_PROBES)),
        }
    }

    async fn probe(&self, path: &str) -> Result<Probe, ProbeError> {
        let _slot = self.slots.acquire().await.map_err(ProbeError::caused_by)?;
        self.probe_media_file.call(path).await
    }
}
