use crate::by_source::BySource;
use domain::Source;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Notify;

#[derive(Clone, Default)]
pub struct Reconciles {
    requested: Arc<Notify>,
    sources: BySource<SourceReconcile>,
}

#[derive(Clone, Default)]
pub(crate) struct SourceReconcile {
    pub(crate) progress: Arc<Mutex<Option<Progress>>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Progress {
    pub(crate) imported: usize,
    pub(crate) total: Option<usize>,
}

impl Reconciles {
    pub async fn requested(&self) {
        self.requested.notified().await;
    }

    pub(crate) fn request(&self) {
        self.requested.notify_one();
    }

    pub(crate) fn source(&self, source: Source) -> &SourceReconcile {
        self.sources.get(source)
    }

    pub(crate) fn progress(&self, source: Source) -> Option<Progress> {
        *self.source(source).progress.lock().unwrap()
    }
}
