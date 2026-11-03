use crate::library::Prober;
use crate::library::SourceHealth;
use crate::library::sync_movie;
use crate::library::sync_show;
use crate::ports::AcquireLock;
use crate::ports::EmailRefused;
use crate::ports::EpisodeFolder;
use crate::ports::EpisodeNfo;
use crate::ports::FindMediaFile;
use crate::ports::FoundMediaFile;
use crate::ports::FoundSubtitleFile;
use crate::ports::GetRadarrMovie;
use crate::ports::GetSonarrShow;
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
use crate::ports::ShowFolder;
use crate::ports::ShowNfo;
use crate::ports::SonarrEpisode;
use crate::ports::SonarrEpisodeFile;
use crate::ports::SonarrShow;
use crate::ports::SonarrShowListing;
use crate::ports::UnreadableNfo;
use crate::ports::persistence;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbLocation;
use jiff::Timestamp;
use jiff::civil::date;
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

pub(crate) fn readable_prober() -> Prober {
    Prober::new(ProbeMediaFile::new(|_| {
        Box::pin(async {
            Ok(Probe {
                container: "matroska,webm".to_owned(),
                duration: None,
                video_streams: Vec::new(),
                audio_streams: Vec::new(),
                subtitle_streams: Vec::new(),
            })
        })
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

pub(crate) const SHOW_FOLDER: &str = "/media/TV/Severance";
pub(crate) const SPECIAL_FILE: &str =
    "/media/TV/Severance/Specials/Severance - S00E01 - Inside the Episode WEBDL-1080p.mkv";
pub(crate) const S01E01_FILE: &str =
    "/media/TV/Severance/Season 01/Severance - S01E01 - Good News About Hell WEBDL-1080p.mkv";
pub(crate) const S01E02_E03_FILE: &str = "/media/TV/Severance/Season 01/Severance - S01E02-E03 - Half Loop + In Perpetuity WEBDL-1080p.mkv";
pub(crate) const S02E01_FILE: &str =
    "/media/TV/Severance/Season 02/Severance - S02E01 - Hello, Ms. Cobel WEBDL-1080p.mkv";

pub(crate) fn sonarr_episode_file(path: &str, date_added: &str) -> Option<SonarrEpisodeFile> {
    Some(SonarrEpisodeFile {
        path: path.to_owned(),
        date_added: at(date_added),
    })
}

pub(crate) fn severance() -> SonarrShow {
    SonarrShow {
        listing: SonarrShowListing {
            sonarr_id: 7,
            tvdb_id: 371_980,
            path: SHOW_FOLDER.to_owned(),
            title: "Severance".to_owned(),
            year: Some(2022),
            genres: vec![
                "Drama".to_owned(),
                "Mystery".to_owned(),
                "Science Fiction".to_owned(),
            ],
            overview: Some("Mark leads a team of office workers.".to_owned()),
        },
        episodes: vec![
            SonarrEpisode {
                sonarr_id: 1001,
                tvdb_id: 9_094_718,
                season_number: 0,
                episode_number: 1,
                title: "Inside the Episode".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Ben Stiller and the cast talk about the first season.".to_owned()),
                episode_file: sonarr_episode_file(SPECIAL_FILE, "2022-03-04T11:02:19Z"),
            },
            SonarrEpisode {
                sonarr_id: 1011,
                tvdb_id: 8_486_245,
                season_number: 1,
                episode_number: 1,
                title: "Good News About Hell".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Mark's memories are divided.".to_owned()),
                episode_file: sonarr_episode_file(S01E01_FILE, "2022-03-01T09:30:00Z"),
            },
            SonarrEpisode {
                sonarr_id: 1012,
                tvdb_id: 8_786_934,
                season_number: 1,
                episode_number: 2,
                title: "Half Loop".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Mark takes Helly on a tour of the office.".to_owned()),
                episode_file: sonarr_episode_file(S01E02_E03_FILE, "2022-03-01T09:31:00Z"),
            },
            SonarrEpisode {
                sonarr_id: 1013,
                tvdb_id: 8_786_935,
                season_number: 1,
                episode_number: 3,
                title: "In Perpetuity".to_owned(),
                air_date: Some(date(2022, 2, 25)),
                overview: Some("Helly takes a tour of the Perpetuity Wing.".to_owned()),
                episode_file: sonarr_episode_file(S01E02_E03_FILE, "2022-03-01T09:31:00Z"),
            },
            SonarrEpisode {
                sonarr_id: 1021,
                tvdb_id: 10_535_541,
                season_number: 2,
                episode_number: 1,
                title: "Hello, Ms. Cobel".to_owned(),
                air_date: Some(date(2025, 1, 17)),
                overview: Some("Mark leads his team on a search for answers.".to_owned()),
                episode_file: sonarr_episode_file(S02E01_FILE, "2025-01-17T03:12:44Z"),
            },
            SonarrEpisode {
                sonarr_id: 1022,
                tvdb_id: 10_535_542,
                season_number: 2,
                episode_number: 2,
                title: "Goodbye, Mrs. Selvig".to_owned(),
                air_date: Some(date(2025, 1, 24)),
                overview: Some("Helly and Mark go looking for Ms. Casey.".to_owned()),
                episode_file: None,
            },
        ],
    }
}

#[derive(Default)]
pub(crate) struct ShowSources {
    pub(crate) sonarr_show: Option<SonarrShow>,
    pub(crate) files_on_disk: HashSet<String>,
    pub(crate) folder: ShowFolder,
    pub(crate) nfo: Option<ShowNfo>,
    pub(crate) nfo_reads: usize,
    pub(crate) episode_folders: HashMap<String, EpisodeFolder>,
    pub(crate) episode_nfos: HashMap<String, Vec<EpisodeNfo>>,
    pub(crate) episode_nfo_reads: usize,
}

#[derive(Clone)]
pub(crate) struct ShowLibrary {
    pub(crate) db_context: DbContext,
    sources: Arc<Mutex<ShowSources>>,
}

impl ShowLibrary {
    pub(crate) async fn new() -> Self {
        let library = Self {
            db_context: mock_db_context().await,
            sources: Arc::default(),
        };
        {
            let mut sources = library.sources();
            sources.sonarr_show = Some(severance());
            sources.files_on_disk = [SPECIAL_FILE, S01E01_FILE, S01E02_E03_FILE, S02E01_FILE]
                .map(str::to_owned)
                .into();
        }
        library
    }

    pub(crate) fn sources(&self) -> MutexGuard<'_, ShowSources> {
        self.sources.lock().unwrap()
    }

    pub(crate) fn sync_show_dependencies(&self) -> sync_show::Dependencies {
        let disk = self.sources.clone();
        let sonarr = self.sources.clone();
        let episode_folders = self.sources.clone();
        let episode_nfos = self.sources.clone();
        let folder = self.sources.clone();
        let nfo = self.sources.clone();
        sync_show::Dependencies {
            db_context: self.db_context.clone(),
            find_media_file: FindMediaFile::new(move |path| {
                let found =
                    disk.lock()
                        .unwrap()
                        .files_on_disk
                        .contains(&path)
                        .then(|| FoundMediaFile {
                            size: 1_000,
                            modified_at: at("2026-10-29T09:00:00Z"),
                        });
                Box::pin(async move { found })
            }),
            get_sonarr_show: GetSonarrShow::new(move |_| {
                let show = sonarr.lock().unwrap().sonarr_show.clone();
                Box::pin(async move { Ok(show) })
            }),
            list_subtitle_files: no_subtitle_files(),
            prober: readable_prober(),
            read_episode_folder: ReadEpisodeFolder::new(move |path| {
                let folder = episode_folders
                    .lock()
                    .unwrap()
                    .episode_folders
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();
                Box::pin(async move { folder })
            }),
            read_episode_nfo: ReadEpisodeNfo::new(move |path| {
                let mut sources = episode_nfos.lock().unwrap();
                sources.episode_nfo_reads += 1;
                let roots = sources
                    .episode_nfos
                    .get(&path)
                    .cloned()
                    .ok_or_else(UnreadableNfo::default);
                Box::pin(async move { roots })
            }),
            read_show_folder: ReadShowFolder::new(move |_, _| {
                let folder = folder.lock().unwrap().folder.clone();
                Box::pin(async move { folder })
            }),
            read_show_nfo: ReadShowNfo::new(move |_| {
                let mut sources = nfo.lock().unwrap();
                sources.nfo_reads += 1;
                let nfo = sources.nfo.clone().ok_or_else(UnreadableNfo::default);
                Box::pin(async move { nfo })
            }),
            source_health: SourceHealth::default(),
        }
    }

    pub(crate) async fn sync_show(&self) -> Result<sync_show::Output, sync_show::Error> {
        sync_show::call(self.sync_show_dependencies(), 7).await
    }
}
