use application::BuildInfo;
use application::browsing::Resizer;
use application::library::Prober;
use application::library::Reconciles;
use application::library::SourceHealth;
use application::ports::AcquireLock;
use application::ports::CheckRadarrKodiMetadata;
use application::ports::CheckSonarrKodiMetadata;
use application::ports::Clock;
use application::ports::DeleteCachedImage;
use application::ports::EndHlsOutput;
use application::ports::ExportTelemetry;
use application::ports::ExportTelemetryError;
use application::ports::FindMediaFile;
use application::ports::GetHlsFile;
use application::ports::GetRadarrMovie;
use application::ports::GetSonarrEpisodes;
use application::ports::GetSonarrShow;
use application::ports::HlsError;
use application::ports::HlsOutput;
use application::ports::ListCachedImages;
use application::ports::ListRadarrMovies;
use application::ports::ListSonarrShows;
use application::ports::ListSubtitleFiles;
use application::ports::LockGuard;
use application::ports::ProbeError;
use application::ports::ProbeMediaFile;
use application::ports::RadarrError;
use application::ports::ReadCachedImage;
use application::ports::ReadEpisodeFolder;
use application::ports::ReadEpisodeNfo;
use application::ports::ReadKeyframes;
use application::ports::ReadMovieFolder;
use application::ports::ReadMovieNfo;
use application::ports::ReadRadarrWebhook;
use application::ports::ReadShowFolder;
use application::ports::ReadShowNfo;
use application::ports::ReadSonarrWebhook;
use application::ports::ReadWebhookEventType;
use application::ports::ResizeImage;
use application::ports::ResizeImageError;
use application::ports::SaveCachedImage;
use application::ports::SendEmail;
use application::ports::SonarrError;
use application::ports::StartHlsOutput;
use application::ports::TryAcquireLock;
use application::ports::UnreadableNfo;
use application::ports::UnreadableWebhook;
use application::ports::persistence;
use application::ports::persistence::DbContext;
use application::ports::persistence::DbLocation;
use application::url_signer::UrlSigner;
use application::webhooks::WebhookPasswords;
use infrastructure::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use test_fixtures::FakeClock;
use test_fixtures::FakeMailer;
use uuid::Uuid;

pub const MAIN_PLAYLIST: &str =
    "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=8000000,RESOLUTION=1920x800\nvideo.m3u8\n";
pub const SEGMENT: &[u8] = b"a ready-made segment";
pub const WEBVTT: &str = "WEBVTT\n\n00:01.000 --> 00:03.000\nA ready-made subtitle\n";

pub struct TestApp {
    pub state: AppState,
    pub clock: FakeClock,
    pub mailer: FakeMailer,
    pub hls: FakeHls,
}

#[derive(Clone)]
pub struct FakeHls {
    segments_dir: Arc<TempDir>,
    outputs: Arc<Mutex<Vec<HlsOutput>>>,
}

impl FakeHls {
    pub fn folder(&self, playback_id: Uuid) -> PathBuf {
        self.segments_dir.path().join(playback_id.to_string())
    }

    pub fn outputs(&self) -> Vec<HlsOutput> {
        self.outputs
            .lock()
            .expect("no test panics while holding the HLS outputs")
            .clone()
    }

    fn start_hls_output(&self) -> StartHlsOutput {
        let hls = self.clone();
        StartHlsOutput::new(move |output| {
            let folder = hls.folder(output.playback_id);
            hls.outputs
                .lock()
                .expect("no test panics while holding the HLS outputs")
                .push(output);
            Box::pin(async move {
                std::fs::create_dir_all(&folder).map_err(HlsError::caused_by)?;
                std::fs::write(folder.join("main.m3u8"), MAIN_PLAYLIST)
                    .map_err(HlsError::caused_by)?;
                std::fs::write(folder.join("0.m4s"), SEGMENT).map_err(HlsError::caused_by)?;
                std::fs::write(folder.join("subtitles-0.vtt"), WEBVTT).map_err(HlsError::caused_by)
            })
        })
    }

    fn get_hls_file(&self) -> GetHlsFile {
        let hls = self.clone();
        GetHlsFile::new(move |playback_id, name| {
            let path = hls.folder(playback_id).join(name);
            Box::pin(async move { Ok(path.exists().then(|| path.to_string_lossy().into_owned())) })
        })
    }

    fn end_hls_output(&self) -> EndHlsOutput {
        let hls = self.clone();
        EndHlsOutput::new(move |playback_id| {
            let _ = std::fs::remove_dir_all(hls.folder(playback_id));
            Box::pin(async {})
        })
    }
}

impl TestApp {
    pub async fn new() -> Self {
        let clock = FakeClock::new();
        let mailer = FakeMailer::default();
        let hls = FakeHls {
            segments_dir: Arc::new(TempDir::new().unwrap()),
            outputs: Arc::default(),
        };
        let state = AppState {
            acquire_lock: AcquireLock::new(|_| Box::pin(async { LockGuard::new(()) })),
            build_info: BuildInfo {
                name: "selfless-test",
                version: "0.0.0-test",
                git_branch: "main",
                git_commit_id: "00000000",
                git_commit_time: "2026-01-01T00:00:00Z",
            },
            check_radarr_kodi_metadata: CheckRadarrKodiMetadata::new(|| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            check_sonarr_kodi_metadata: CheckSonarrKodiMetadata::new(|| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            clock: Clock::new(clock.reader()),
            db_context: DbContext::new(
                test_fixtures::mock_database(persistence::models()).await,
                DbLocation {
                    database: "mock".to_owned(),
                    address: "localhost".to_owned(),
                    port: 0,
                },
            ),
            delete_cached_image: DeleteCachedImage::new(|_| Box::pin(async { Ok(()) })),
            end_hls_output: hls.end_hls_output(),
            export_telemetry: ExportTelemetry::new(|_, _| {
                Box::pin(async { Err(ExportTelemetryError::default()) })
            }),
            find_media_file: FindMediaFile::new(|_| Box::pin(async { Ok(None) })),
            get_hls_file: hls.get_hls_file(),
            get_radarr_movie: GetRadarrMovie::new(|_| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            get_sonarr_episodes: GetSonarrEpisodes::new(|_| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            get_sonarr_show: GetSonarrShow::new(|_| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            list_cached_images: ListCachedImages::new(|| Box::pin(async { Ok(Vec::new()) })),
            list_radarr_movies: ListRadarrMovies::new(|| {
                Box::pin(async { Err(RadarrError::default()) })
            }),
            list_sonarr_shows: ListSonarrShows::new(|| {
                Box::pin(async { Err(SonarrError::default()) })
            }),
            list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
            prober: Prober::new(ProbeMediaFile::new(|_| {
                Box::pin(async { Err(ProbeError::default()) })
            })),
            read_cached_image: ReadCachedImage::new(|_| Box::pin(async { Ok(None) })),
            read_episode_folder: ReadEpisodeFolder::new(|_| {
                Box::pin(async { Ok(Default::default()) })
            }),
            read_episode_nfo: ReadEpisodeNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default().into()) })
            }),
            read_keyframes: ReadKeyframes::new(|_| {
                Box::pin(async { Ok((0..2000).map(|x| Duration::from_secs(x * 6)).collect()) })
            }),
            read_movie_folder: ReadMovieFolder::new(|_| Box::pin(async { Ok(Default::default()) })),
            read_movie_nfo: ReadMovieNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default().into()) })
            }),
            read_radarr_webhook: ReadRadarrWebhook::new(|_| Err(UnreadableWebhook::default())),
            read_show_folder: ReadShowFolder::new(|_, _| {
                Box::pin(async { Ok(Default::default()) })
            }),
            read_show_nfo: ReadShowNfo::new(|_| {
                Box::pin(async { Err(UnreadableNfo::default().into()) })
            }),
            read_sonarr_webhook: ReadSonarrWebhook::new(|_| Err(UnreadableWebhook::default())),
            read_webhook_event_type: ReadWebhookEventType::new(|_| None),
            reconciles: Reconciles::default(),
            resizer: Resizer::new(ResizeImage::new(|_, _| {
                Box::pin(async { Err(ResizeImageError::default()) })
            })),
            save_cached_image: SaveCachedImage::new(|_, _| Box::pin(async { Ok(()) })),
            send_email: SendEmail::new(mailer.sender()),
            source_health: SourceHealth::default(),
            start_hls_output: hls.start_hls_output(),
            try_acquire_lock: TryAcquireLock::new(|_| Box::pin(async { Some(LockGuard::new(())) })),
            url_signer: UrlSigner::new(b"a household secret"),
            webhook_passwords: WebhookPasswords::default(),
        };
        Self {
            state,
            clock,
            mailer,
            hls,
        }
    }
}
