use crate::browsing::Resizer;
use crate::library::Prober;
use crate::library::SourceHealth;
use crate::library::sync_movie;
use crate::library::sync_show;
use crate::ports::AcquireLock;
use crate::ports::CachedImage;
use crate::ports::Clock;
use crate::ports::DeleteCachedImage;
use crate::ports::EmailRefused;
use crate::ports::EndHlsOutput;
use crate::ports::EpisodeFolder;
use crate::ports::FindMediaFile;
use crate::ports::FoundFile;
use crate::ports::FoundMediaFile;
use crate::ports::FoundSubtitleFile;
use crate::ports::GetRadarrMovie;
use crate::ports::GetSonarrShow;
use crate::ports::HlsOutput;
use crate::ports::ImageFile;
use crate::ports::ImageFormat;
use crate::ports::ListCachedImages;
use crate::ports::ListSubtitleFiles;
use crate::ports::LockGuard;
use crate::ports::MovieFolder;
use crate::ports::Probe;
use crate::ports::ProbeError;
use crate::ports::ProbeMediaFile;
use crate::ports::RadarrError;
use crate::ports::ReadCachedImage;
use crate::ports::ReadEpisodeFolder;
use crate::ports::ReadEpisodeNfo;
use crate::ports::ReadMovieFolder;
use crate::ports::ReadMovieNfo;
use crate::ports::ReadShowFolder;
use crate::ports::ReadShowNfo;
use crate::ports::ResizeImage;
use crate::ports::SaveCachedImage;
use crate::ports::SendEmail;
use crate::ports::SendEmailError;
use crate::ports::ShowFolder;
use crate::ports::SourceEpisode;
use crate::ports::SourceMediaFile;
use crate::ports::SourceMovie;
use crate::ports::SourceShow;
use crate::ports::SourceShowListing;
use crate::ports::StartHlsOutput;
use crate::ports::UnreadableFile;
use crate::ports::UnreadableNfo;
use crate::ports::persistence;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbLocation;
use crate::ports::persistence::DbScope;
use crate::url_signer::UrlSigner;
use crate::watch_progress::record_progress;
use domain::Episode;
use domain::EpisodeNfo;
use domain::Movie;
use domain::MovieNfo;
use domain::PlayMethod;
use domain::PlayReasonKind;
use domain::Playback;
use domain::RadarrId;
use domain::Show;
use domain::ShowNfo;
use domain::SonarrId;
use domain::TmdbId;
use domain::TvdbId;
use jiff::SignedDuration;
use jiff::Timestamp;
use jiff::civil::date;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use test_fixtures::FakeClock;
use toasty::schema::Model;
use tokio::sync::watch;
use uuid::Uuid;

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

pub(crate) fn url_signer() -> UrlSigner {
    UrlSigner::new(b"a household secret")
}

pub(crate) async fn movie_titles(db_context: &DbContext) -> Vec<String> {
    let mut db_context = db_context.clone();
    let mut titles = db_context
        .movies()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.title)
        .collect::<Vec<_>>();
    titles.sort();
    titles
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

#[derive(Clone, Default)]
pub(crate) struct EndedHlsOutputs {
    playback_ids: Arc<Mutex<Vec<Uuid>>>,
}

impl EndedHlsOutputs {
    pub(crate) fn port(&self) -> EndHlsOutput {
        let playback_ids = self.playback_ids.clone();
        EndHlsOutput::new(move |playback_id| {
            playback_ids.lock().unwrap().push(playback_id);
            Box::pin(async {})
        })
    }

    pub(crate) fn playback_ids(&self) -> Vec<Uuid> {
        self.playback_ids.lock().unwrap().clone()
    }
}

#[derive(Clone, Default)]
pub(crate) struct StartedHlsOutputs {
    outputs: Arc<Mutex<Vec<HlsOutput>>>,
}

impl StartedHlsOutputs {
    pub(crate) fn port(&self) -> StartHlsOutput {
        let outputs = self.outputs.clone();
        StartHlsOutput::new(move |output| {
            outputs.lock().unwrap().push(output);
            Box::pin(async { Ok(()) })
        })
    }

    pub(crate) fn last(&self) -> HlsOutput {
        self.outputs
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("an HLS output started")
    }

    pub(crate) fn count(&self) -> usize {
        self.outputs.lock().unwrap().len()
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
    FindMediaFile::new(|_| Box::pin(async { Ok(None) }))
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
    ReadEpisodeFolder::new(|_| Box::pin(async { Ok(Default::default()) }))
}

pub(crate) fn unreadable_episode_nfo() -> ReadEpisodeNfo {
    ReadEpisodeNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default().into()) }))
}

pub(crate) fn empty_movie_folder() -> ReadMovieFolder {
    ReadMovieFolder::new(|_| Box::pin(async { Ok(Default::default()) }))
}

pub(crate) fn unreadable_movie_nfo() -> ReadMovieNfo {
    ReadMovieNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default().into()) }))
}

pub(crate) fn empty_show_folder() -> ReadShowFolder {
    ReadShowFolder::new(|_, _| Box::pin(async { Ok(Default::default()) }))
}

pub(crate) fn unreadable_show_nfo() -> ReadShowNfo {
    ReadShowNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default().into()) }))
}

fn readable<'a>(
    unreadable_files: &HashSet<String>,
    files: impl IntoIterator<Item = &'a FoundFile>,
) -> Result<(), UnreadableFile> {
    match files
        .into_iter()
        .find(|x| unreadable_files.contains(&x.path))
    {
        Some(file) => Err(UnreadableFile::new(file.path.clone())),
        None => Ok(()),
    }
}

fn random_id() -> i64 {
    Uuid::new_v4().as_u64_pair().0 as i64
}

pub(crate) fn movie(db_context: &DbContext, title: &str) -> <Movie as Model>::Create {
    db_context
        .movies()
        .create()
        .tmdb_id(TmdbId(random_id()))
        .radarr_id(RadarrId(random_id()))
        .title(title)
        .year(2016)
        .genres(Vec::<String>::new())
        .directors(Vec::<String>::new())
        .in_library(true)
}

pub(crate) fn show(db_context: &DbContext, title: &str) -> <Show as Model>::Create {
    db_context
        .shows()
        .create()
        .tvdb_id(TvdbId(random_id()))
        .sonarr_id(SonarrId(random_id()))
        .title(title)
        .year(Some(2022))
        .genres(Vec::<String>::new())
}

pub(crate) async fn add_season(db_context: &mut DbContext, show_id: Uuid, number: i64) -> Uuid {
    db_context
        .seasons()
        .create()
        .show_id(show_id)
        .number(number)
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}

pub(crate) fn episode(db_context: &DbContext, season_id: Uuid) -> <Episode as Model>::Create {
    db_context
        .episodes()
        .create()
        .season_id(season_id)
        .tvdb_id(TvdbId(random_id()))
        .number(1)
        .title("Good News About Hell")
        .in_library(true)
}

pub(crate) fn playback(
    db_context: &DbContext,
    last_reported_at: Timestamp,
) -> <Playback as Model>::Create {
    db_context
        .playbacks()
        .create()
        .user_id(Uuid::new_v4())
        .session_id(Uuid::new_v4())
        .media_file_id(Uuid::new_v4())
        .method(PlayMethod::DirectPlay)
        .position(0.0)
        .duration(6981.0)
        .paused(false)
        .started_at(last_reported_at)
        .last_reported_at(last_reported_at)
}

pub(crate) async fn add_playback_reason(db_context: &mut DbContext, playback_id: Uuid) {
    db_context
        .playback_reasons()
        .create()
        .playback_id(playback_id)
        .index(0)
        .kind(PlayReasonKind::Container)
        .value(Some("mkv".to_owned()))
        .exec(db_context.executor_mut())
        .await
        .unwrap();
}

pub(crate) async fn playback_reason_owners(db_context: &mut DbContext) -> Vec<Uuid> {
    let mut owners = db_context
        .playback_reasons()
        .all()
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.playback_id)
        .collect::<Vec<_>>();
    owners.sort();
    owners
}

pub(crate) async fn add_media_file(db_context: &DbContext, duration_ms: Option<i64>) -> Uuid {
    let mut db_context = db_context.clone();
    db_context
        .media_files()
        .create()
        .path(format!("/media/{}.mkv", Uuid::new_v4()))
        .duration_ms(duration_ms)
        .exec(db_context.executor_mut())
        .await
        .unwrap()
        .id
}

const REPORTED_DURATION: SignedDuration = SignedDuration::from_mins(50);

pub(crate) async fn stop_at(
    db_context: &DbContext,
    user_id: Uuid,
    media_file_id: Uuid,
    position: SignedDuration,
    reported_at: &str,
) {
    let clock = FakeClock::new();
    clock.set(at(reported_at));
    record_progress::call(
        record_progress::Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(clock.reader()),
        },
        user_id,
        media_file_id,
        position,
        REPORTED_DURATION,
    )
    .await
    .unwrap();
}

pub(crate) async fn watch_to_the_end(
    db_context: &DbContext,
    user_id: Uuid,
    media_file_id: Uuid,
    reported_at: &str,
) {
    stop_at(
        db_context,
        user_id,
        media_file_id,
        REPORTED_DURATION,
        reported_at,
    )
    .await;
}

pub(crate) const MOVIE_FILE: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mkv";

pub(crate) fn at(timestamp: &str) -> Timestamp {
    timestamp.parse().expect("the timestamp is valid")
}

pub(crate) fn arrival(file: Option<&str>) -> SourceMovie {
    SourceMovie {
        radarr_id: RadarrId(42),
        tmdb_id: TmdbId(329_865),
        title: "Arrival".to_owned(),
        year: 2016,
        overview: Some("An expert linguist is recruited by the military.".to_owned()),
        genres: vec!["Drama".to_owned(), "Science Fiction".to_owned()],
        media_file: file.map(|path| SourceMediaFile {
            path: path.to_owned(),
            added_at: at("2026-10-29T10:00:00Z"),
        }),
    }
}

#[derive(Default)]
pub(crate) struct MovieSources {
    pub(crate) movie: Option<SourceMovie>,
    pub(crate) radarr_is_down: bool,
    pub(crate) files_on_disk: HashMap<String, FoundMediaFile>,
    pub(crate) unreadable_files: HashSet<String>,
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
        library.sources().movie = Some(arrival(Some(MOVIE_FILE)));
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
            let sources = sources.lock().unwrap();
            let found = if sources.unreadable_files.contains(&path) {
                Err(UnreadableFile::new(path))
            } else {
                Ok(sources.files_on_disk.get(&path).copied())
            };
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
                    Ok(sources.movie.clone())
                };
                Box::pin(async move { movie })
            }),
            list_subtitle_files: self.list_subtitle_files(),
            prober: self.prober(),
            read_movie_folder: ReadMovieFolder::new(move |_| {
                let sources = folder.lock().unwrap();
                let folder = &sources.folder;
                let folder = readable(
                    &sources.unreadable_files,
                    [&folder.poster, &folder.fanart, &folder.clearlogo]
                        .into_iter()
                        .flatten(),
                )
                .map(|()| folder.clone());
                Box::pin(async move { folder })
            }),
            read_movie_nfo: ReadMovieNfo::new(move |path| {
                let mut sources = nfo.lock().unwrap();
                sources.nfo_reads += 1;
                let nfo = if sources.unreadable_files.contains(&path) {
                    Err(UnreadableFile::new(path).into())
                } else {
                    sources
                        .nfo
                        .clone()
                        .ok_or_else(|| UnreadableNfo::default().into())
                };
                Box::pin(async move { nfo })
            }),
            source_health: self.source_health.clone(),
        }
    }

    pub(crate) async fn sync_movie(&self) -> Result<sync_movie::Output, sync_movie::Error> {
        sync_movie::call(self.sync_movie_dependencies(), RadarrId(42)).await
    }
}

#[derive(Clone, Default)]
pub(crate) struct FakeImageCache {
    files: Arc<Mutex<HashMap<CachedImage, ImageFile>>>,
}

impl FakeImageCache {
    pub(crate) fn read(&self) -> ReadCachedImage {
        let files = self.files.clone();
        ReadCachedImage::new(move |image| {
            let file = files.lock().unwrap().get(&image).cloned();
            Box::pin(async move { Ok(file) })
        })
    }

    pub(crate) fn save(&self) -> SaveCachedImage {
        let files = self.files.clone();
        SaveCachedImage::new(move |image, file| {
            files.lock().unwrap().insert(image, file);
            Box::pin(async { Ok(()) })
        })
    }

    pub(crate) fn list(&self) -> ListCachedImages {
        let files = self.files.clone();
        ListCachedImages::new(move || {
            let images = files.lock().unwrap().keys().cloned().collect();
            Box::pin(async move { Ok(images) })
        })
    }

    pub(crate) fn delete(&self) -> DeleteCachedImage {
        let files = self.files.clone();
        DeleteCachedImage::new(move |image| {
            files.lock().unwrap().remove(&image);
            Box::pin(async { Ok(()) })
        })
    }

    pub(crate) fn images(&self) -> Vec<CachedImage> {
        let mut images = self
            .files
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        images.sort_by_key(|x| (x.image_id, x.width, x.modified_at));
        images
    }
}

#[derive(Clone)]
pub(crate) struct FakeResizer {
    state: Arc<Mutex<ResizerState>>,
    held: watch::Sender<bool>,
}

#[derive(Default)]
struct ResizerState {
    resized: Vec<(String, i64)>,
    running: usize,
    most_running: usize,
}

impl FakeResizer {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::default(),
            held: watch::Sender::new(false),
        }
    }

    pub(crate) fn resizer(&self) -> Resizer {
        let fake = self.clone();
        Resizer::new(ResizeImage::new(move |path, width| {
            let fake = fake.clone();
            Box::pin(async move {
                {
                    let mut state = fake.state.lock().unwrap();
                    state.resized.push((path.clone(), width));
                    state.running += 1;
                    state.most_running = state.most_running.max(state.running);
                }
                let _ = fake.held.subscribe().wait_for(|held| !held).await;
                fake.state.lock().unwrap().running -= 1;
                Ok(ImageFile {
                    format: ImageFormat::Jpeg,
                    bytes: format!("{path} at {width}").into(),
                })
            })
        }))
    }

    pub(crate) fn resized(&self) -> Vec<(String, i64)> {
        self.state.lock().unwrap().resized.clone()
    }

    pub(crate) fn running(&self) -> usize {
        self.state.lock().unwrap().running
    }

    pub(crate) fn most_running(&self) -> usize {
        self.state.lock().unwrap().most_running
    }

    pub(crate) fn hold(&self) {
        self.held.send_replace(true);
    }

    pub(crate) fn release(&self) {
        self.held.send_replace(false);
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

pub(crate) fn source_media_file(path: &str, added_at: &str) -> Option<SourceMediaFile> {
    Some(SourceMediaFile {
        path: path.to_owned(),
        added_at: at(added_at),
    })
}

pub(crate) fn severance() -> SourceShow {
    SourceShow {
        listing: SourceShowListing {
            sonarr_id: SonarrId(7),
            tvdb_id: TvdbId(371_980),
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
            SourceEpisode {
                sonarr_id: SonarrId(1001),
                tvdb_id: TvdbId(9_094_718),
                season_number: 0,
                episode_number: 1,
                title: "Inside the Episode".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Ben Stiller and the cast talk about the first season.".to_owned()),
                media_file: source_media_file(SPECIAL_FILE, "2022-03-04T11:02:19Z"),
            },
            SourceEpisode {
                sonarr_id: SonarrId(1011),
                tvdb_id: TvdbId(8_486_245),
                season_number: 1,
                episode_number: 1,
                title: "Good News About Hell".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Mark's memories are divided.".to_owned()),
                media_file: source_media_file(S01E01_FILE, "2022-03-01T09:30:00Z"),
            },
            SourceEpisode {
                sonarr_id: SonarrId(1012),
                tvdb_id: TvdbId(8_786_934),
                season_number: 1,
                episode_number: 2,
                title: "Half Loop".to_owned(),
                air_date: Some(date(2022, 2, 18)),
                overview: Some("Mark takes Helly on a tour of the office.".to_owned()),
                media_file: source_media_file(S01E02_E03_FILE, "2022-03-01T09:31:00Z"),
            },
            SourceEpisode {
                sonarr_id: SonarrId(1013),
                tvdb_id: TvdbId(8_786_935),
                season_number: 1,
                episode_number: 3,
                title: "In Perpetuity".to_owned(),
                air_date: Some(date(2022, 2, 25)),
                overview: Some("Helly takes a tour of the Perpetuity Wing.".to_owned()),
                media_file: source_media_file(S01E02_E03_FILE, "2022-03-01T09:31:00Z"),
            },
            SourceEpisode {
                sonarr_id: SonarrId(1021),
                tvdb_id: TvdbId(10_535_541),
                season_number: 2,
                episode_number: 1,
                title: "Hello, Ms. Cobel".to_owned(),
                air_date: Some(date(2025, 1, 17)),
                overview: Some("Mark leads his team on a search for answers.".to_owned()),
                media_file: source_media_file(S02E01_FILE, "2025-01-17T03:12:44Z"),
            },
            SourceEpisode {
                sonarr_id: SonarrId(1022),
                tvdb_id: TvdbId(10_535_542),
                season_number: 2,
                episode_number: 2,
                title: "Goodbye, Mrs. Selvig".to_owned(),
                air_date: Some(date(2025, 1, 24)),
                overview: Some("Helly and Mark go looking for Ms. Casey.".to_owned()),
                media_file: None,
            },
        ],
    }
}

#[derive(Default)]
pub(crate) struct ShowSources {
    pub(crate) show: Option<SourceShow>,
    pub(crate) files_on_disk: HashSet<String>,
    pub(crate) unreadable_files: HashSet<String>,
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
            sources.show = Some(severance());
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
                let sources = disk.lock().unwrap();
                let found = if sources.unreadable_files.contains(&path) {
                    Err(UnreadableFile::new(path))
                } else {
                    Ok(sources
                        .files_on_disk
                        .contains(&path)
                        .then(|| FoundMediaFile {
                            size: 1_000,
                            modified_at: at("2026-10-29T09:00:00Z"),
                        }))
                };
                Box::pin(async move { found })
            }),
            get_sonarr_show: GetSonarrShow::new(move |_| {
                let show = sonarr.lock().unwrap().show.clone();
                Box::pin(async move { Ok(show) })
            }),
            list_subtitle_files: no_subtitle_files(),
            prober: readable_prober(),
            read_episode_folder: ReadEpisodeFolder::new(move |path| {
                let sources = episode_folders.lock().unwrap();
                let folder = sources
                    .episode_folders
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();
                let folder =
                    readable(&sources.unreadable_files, folder.thumb.iter()).map(|()| folder);
                Box::pin(async move { folder })
            }),
            read_episode_nfo: ReadEpisodeNfo::new(move |path| {
                let mut sources = episode_nfos.lock().unwrap();
                sources.episode_nfo_reads += 1;
                let roots = if sources.unreadable_files.contains(&path) {
                    Err(UnreadableFile::new(path).into())
                } else {
                    sources
                        .episode_nfos
                        .get(&path)
                        .cloned()
                        .ok_or_else(|| UnreadableNfo::default().into())
                };
                Box::pin(async move { roots })
            }),
            read_show_folder: ReadShowFolder::new(move |_, _| {
                let sources = folder.lock().unwrap();
                let folder = &sources.folder;
                let folder = readable(
                    &sources.unreadable_files,
                    [&folder.poster, &folder.fanart, &folder.clearlogo]
                        .into_iter()
                        .flatten()
                        .chain(folder.season_posters.iter().map(|(_, x)| x)),
                )
                .map(|()| folder.clone());
                Box::pin(async move { folder })
            }),
            read_show_nfo: ReadShowNfo::new(move |path| {
                let mut sources = nfo.lock().unwrap();
                sources.nfo_reads += 1;
                let nfo = if sources.unreadable_files.contains(&path) {
                    Err(UnreadableFile::new(path).into())
                } else {
                    sources
                        .nfo
                        .clone()
                        .ok_or_else(|| UnreadableNfo::default().into())
                };
                Box::pin(async move { nfo })
            }),
            source_health: SourceHealth::default(),
        }
    }

    pub(crate) async fn sync_show(&self) -> Result<sync_show::Output, sync_show::Error> {
        sync_show::call(self.sync_show_dependencies(), SonarrId(7)).await
    }
}
