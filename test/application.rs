use crate::MediaFolder;
use crate::Prober;
use crate::ports::AcquireLock;
use crate::ports::EmailRefused;
use crate::ports::FindMediaFile;
use crate::ports::ListSubtitleFiles;
use crate::ports::LockGuard;
use crate::ports::ProbeError;
use crate::ports::ProbeMediaFile;
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
use std::sync::Arc;
use std::sync::Mutex;

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

pub(crate) fn empty_media_folder() -> MediaFolder {
    MediaFolder {
        find_media_file: FindMediaFile::new(|_| Box::pin(async { None })),
        list_subtitle_files: ListSubtitleFiles::new(|_| Box::pin(async { Ok(Vec::new()) })),
        prober: Prober::new(ProbeMediaFile::new(|_| {
            Box::pin(async { Err(ProbeError::default()) })
        })),
        read_episode_folder: ReadEpisodeFolder::new(|_| Box::pin(async { Default::default() })),
        read_episode_nfo: ReadEpisodeNfo::new(|_| {
            Box::pin(async { Err(UnreadableNfo::default()) })
        }),
        read_movie_folder: ReadMovieFolder::new(|_| Box::pin(async { Default::default() })),
        read_movie_nfo: ReadMovieNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) })),
        read_show_folder: ReadShowFolder::new(|_, _| Box::pin(async { Default::default() })),
        read_show_nfo: ReadShowNfo::new(|_| Box::pin(async { Err(UnreadableNfo::default()) })),
    }
}
