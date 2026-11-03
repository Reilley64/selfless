use crate::library::Prober;
use crate::library::SourceHealth;
use crate::library::sync_movie;
use crate::ports::AcquireLock;
use crate::ports::EmailRefused;
use crate::ports::FindMediaFile;
use crate::ports::FoundMediaFile;
use crate::ports::FoundSubtitleFile;
use crate::ports::GetRadarrMovie;
use crate::ports::ListSubtitleFiles;
use crate::ports::LockGuard;
use crate::ports::MovieFolder;
use crate::ports::MovieNfo;
use crate::ports::Probe;
use crate::ports::ProbeError;
use crate::ports::ProbeMediaFile;
use crate::ports::RadarrError;
use crate::ports::RadarrMovie;
use crate::ports::RadarrMovieFile;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::SendEmail;
use crate::ports::SendEmailError;
use crate::ports::UnreadableNfo;
use crate::ports::persistence;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbLocation;
use jiff::Timestamp;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;

pub(crate) async fn mock_db_context() -> DbContext {
    DbContext::new(
        test_fixtures::mock_database(persistence::models()).await,
        DbLocation {
            database: "mock".to_owned(),
            address: "localhost".to_owned(),
            port: 0,
        },
    )
}

#[derive(Clone, Default)]
pub(crate) struct RecordingLock {
    keys: Arc<Mutex<Vec<String>>>,
}

impl RecordingLock {
    pub(crate) fn port(&self) -> AcquireLock {
        let keys = self.keys.clone();
        AcquireLock::new(move |key| {
            keys.lock().unwrap().push(key);
            Box::pin(async { LockGuard::new(()) })
        })
    }

    pub(crate) fn keys(&self) -> Vec<String> {
        self.keys.lock().unwrap().clone()
    }
}

pub(crate) fn failing_mailer() -> SendEmail {
    SendEmail::new(|_, _, _| {
        Box::pin(async { Err(SendEmailError::caused_by("the SMTP server is down")) })
    })
}

pub(crate) fn refusing_mailer() -> SendEmail {
    SendEmail::new(|_, _, _| {
        Box::pin(async { Err(SendEmailError::caused_by(EmailRefused::new())) })
    })
}

pub(crate) fn missing_media_file() -> FindMediaFile {
    FindMediaFile::new(|_| Box::pin(async { None }))
}

pub(crate) fn no_subtitle_files() -> ListSubtitleFiles {
    ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) }))
}

pub(crate) fn failing_prober() -> Prober {
    Prober::new(ProbeMediaFile::new(|_| {
        Box::pin(async { Err(ProbeError::default()) })
    }))
}

pub(crate) fn empty_episode_folder() -> ReadEpisodeFolder {
    ReadEpisodeFolder::new(|_| Box::pin(async { Default::default() }))
}

pub(crate) fn unreadable_episode_nfo() -> ReadEpisodeNfo {
    ReadEpisodeNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) }))
}

pub(crate) fn empty_movie_folder() -> ReadMovieFolder {
    ReadMovieFolder::new(|_| Box::pin(async { Default::default() }))
}

pub(crate) fn unreadable_movie_nfo() -> ReadMovieNfo {
    ReadMovieNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) }))
}

pub(crate) fn empty_show_folder() -> ReadShowFolder {
    ReadShowFolder::new(|_, _| Box::pin(async { Default::default() }))
}

pub(crate) fn unreadable_show_nfo() -> ReadShowNfo {
    ReadShowNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) }))
}

pub(crate) const MOVIE_FILE: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";

pub(crate) fn at(timestamp: &str) -> Timestamp {
    timestamp.parse().expect("the timestamp is valid")
}

pub(crate) fn arrival(file: Option<&str>) -> RadarrMovie {
    RadarrMovie {
        radarr_id: 42,
        tmdb_id: 329_865,
        title: "Arrival".to_owned(),
        year: 2016,
        overview: Some("An expert linguist is recruited by the military.".to_owned()),
        genres: vec!["Drama".to_owned(), "Science Fiction".to_owned()],
        movie_file: file.map(|path| RadarrMovieFile {
            path: path.to_owned(),
            date_added: at("2026-10-29T10:00:00Z"),
        }),
    }
}

#[derive(Default)]
pub(crate) struct MovieSources {
    pub(crate) radarr_movie: Option<RadarrMovie>,
    pub(crate) radarr_is_down: bool,
    pub(crate) files_on_disk: HashMap<String, FoundMediaFile>,
    pub(crate) subtitle_files: Vec<FoundSubtitleFile>,
    pub(crate) folder: MovieFolder,
    pub(crate) nfo: Option<MovieNfo>,
    pub(crate) nfo_reads: usize,
    pub(crate) probes: HashMap<String, Probe>,
    pub(crate) unprobeable: HashSet<String>,
    pub(crate) probe_calls: usize,
}

#[derive(Clone)]
pub(crate) struct MovieLibrary {
    pub(crate) db_context: DbContext,
    pub(crate) source_health: SourceHealth,
    sources: Arc<Mutex<MovieSources>>,
}

impl MovieLibrary {
    pub(crate) async fn new() -> Self {
        let library = Self {
            db_context: mock_db_context().await,
            source_health: SourceHealth::default(),
            sources: Arc::default(),
        };
        library.sources().radarr_movie = Some(arrival(Some(MOVIE_FILE)));
        library.put_file(MOVIE_FILE, 1_000, "2026-10-29T09:00:00Z");
        library
    }

    pub(crate) fn sources(&self) -> MutexGuard<'_, MovieSources> {
        self.sources.lock().unwrap()
    }

    pub(crate) fn put_file(&self, path: &str, size: i64, modified_at: &str) {
        self.sources().files_on_disk.insert(
            path.to_owned(),
            FoundMediaFile {
                size,
                modified_at: at(modified_at),
            },
        );
    }

    pub(crate) fn find_media_file(&self) -> FindMediaFile {
        let sources = self.sources.clone();
        FindMediaFile::new(move |path| {
            let found = sources.lock().unwrap().files_on_disk.get(&path).copied();
            Box::pin(async move { found })
        })
    }

    pub(crate) fn list_subtitle_files(&self) -> ListSubtitleFiles {
        let sources = self.sources.clone();
        ListSubtitleFiles::new(move |_| {
            let subtitle_files = sources.lock().unwrap().subtitle_files.clone();
            Box::pin(async move { Ok(subtitle_files) })
        })
    }

    pub(crate) fn prober(&self) -> Prober {
        let sources = self.sources.clone();
        Prober::new(ProbeMediaFile::new(move |path| {
            let mut sources = sources.lock().unwrap();
            sources.probe_calls += 1;
            let probe = if sources.unprobeable.contains(&path) {
                Err(ProbeError::caused_by("ffprobe cannot read the file"))
            } else {
                Ok(sources.probes.get(&path).cloned().unwrap_or(Probe {
                    container: "matroska,webm".to_owned(),
                    duration: None,
                    video_streams: Vec::new(),
                    audio_streams: Vec::new(),
                    subtitle_streams: Vec::new(),
                }))
            };
            Box::pin(async move { probe })
        }))
    }

    pub(crate) fn sync_movie_dependencies(&self) -> sync_movie::Dependencies {
        let radarr = self.sources.clone();
        let folder = self.sources.clone();
        let nfo = self.sources.clone();
        sync_movie::Dependencies {
            db_context: self.db_context.clone(),
            find_media_file: self.find_media_file(),
            get_radarr_movie: GetRadarrMovie::new(move |_| {
                let sources = radarr.lock().unwrap();
                let movie = if sources.radarr_is_down {
                    Err(RadarrError::caused_by("Radarr is down"))
                } else {
                    Ok(sources.radarr_movie.clone())
                };
                Box::pin(async move { movie })
            }),
            list_subtitle_files: self.list_subtitle_files(),
            prober: self.prober(),
            read_movie_folder: ReadMovieFolder::new(move |_| {
                let folder = folder.lock().unwrap().folder.clone();
                Box::pin(async move { folder })
            }),
            read_movie_nfo: ReadMovieNfo::new(move |_| {
                let mut sources = nfo.lock().unwrap();
                sources.nfo_reads += 1;
                let nfo = sources.nfo.clone().ok_or_else(UnreadableNfo::default);
                Box::pin(async move { nfo })
            }),
            source_health: self.source_health.clone(),
        }
    }

    pub(crate) async fn sync_movie(&self) -> Result<sync_movie::Output, sync_movie::Error> {
        sync_movie::call(self.sync_movie_dependencies(), 42).await
    }
}
