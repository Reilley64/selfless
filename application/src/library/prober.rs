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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::poll;
    use std::sync::Mutex;

    #[tokio::test]
    async fn a_fifth_probe_starts_only_once_one_of_four_running_probes_finishes() {
        let started = Arc::new(Mutex::new(Vec::new()));
        let finish = Arc::new(Semaphore::new(0));
        let prober = Prober::new(ProbeMediaFile::new({
            let started = started.clone();
            let finish = finish.clone();
            move |path| {
                started.lock().unwrap().push(path);
                let finish = finish.clone();
                Box::pin(async move {
                    finish.acquire().await.unwrap().forget();
                    Ok(Probe {
                        container: "matroska,webm".to_owned(),
                        duration: None,
                        video_streams: Vec::new(),
                        audio_streams: Vec::new(),
                        subtitle_streams: Vec::new(),
                    })
                })
            }
        }));
        let paths = (0..5).map(|x| format!("{x}.mkv")).collect::<Vec<_>>();
        let mut probes = paths
            .iter()
            .map(|path| Box::pin(prober.probe(path)))
            .collect::<Vec<_>>();
        for probe in &mut probes {
            assert!(poll!(probe).is_pending());
        }
        assert_eq!(*started.lock().unwrap(), paths[..4]);

        finish.add_permits(1);
        assert!(poll!(&mut probes[0]).is_ready());
        assert!(poll!(&mut probes[4]).is_pending());

        assert_eq!(*started.lock().unwrap(), paths);
    }
}
