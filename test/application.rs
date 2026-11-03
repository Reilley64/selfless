use crate::browsing::Resizer;
use crate::library::Prober;
use crate::ports::AcquireLock;
use crate::ports::CachedImage;
use crate::ports::DeleteCachedImage;
use crate::ports::EmailRefused;
use crate::ports::FindMediaFile;
use crate::ports::ImageFile;
use crate::ports::ImageFormat;
use crate::ports::ListCachedImages;
use crate::ports::ListSubtitleFiles;
use crate::ports::LockGuard;
use crate::ports::ProbeError;
use crate::ports::ProbeMediaFile;
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
use crate::ports::UnreadableNfo;
use crate::ports::persistence;
use crate::ports::persistence::DbContext;
use crate::ports::persistence::DbLocation;
use crate::url_signer::UrlSigner;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::watch;

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
