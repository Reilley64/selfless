use crate::ports::AcquireLock;
use crate::ports::EmailRejected;
use crate::ports::LockGuard;
use crate::ports::SendEmail;
use crate::ports::SendEmailError;
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
        Box::pin(async { Err(SendEmailError::caused_by(EmailRejected::new())) })
    })
}
