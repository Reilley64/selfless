use crate::ports::Probe;
use crate::ports::ProbeError;
use crate::ports::ProbeMediaFile;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(crate) const CONCURRENT_PROBES: usize = 4;

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

    pub(crate) async fn probe(&self, path: &str) -> Result<Probe, ProbeError> {
        let _slot = self.slots.acquire().await.map_err(ProbeError::caused_by)?;
        self.probe_media_file.call(path).await
    }
}
