use crate::ports::ImageFile;
use crate::ports::ResizeImage;
use crate::ports::ResizeImageError;
use std::sync::Arc;
use tokio::sync::Semaphore;

const CONCURRENT_RESIZES: usize = 4;

#[derive(Clone)]
pub struct Resizer {
    resize_image: ResizeImage,
    slots: Arc<Semaphore>,
}

impl Resizer {
    pub fn new(resize_image: ResizeImage) -> Self {
        Self {
            resize_image,
            slots: Arc::new(Semaphore::new(CONCURRENT_RESIZES)),
        }
    }

    pub(crate) async fn resize(
        &self,
        path: &str,
        width: i64,
    ) -> Result<ImageFile, ResizeImageError> {
        let _slot = self
            .slots
            .acquire()
            .await
            .map_err(ResizeImageError::caused_by)?;
        self.resize_image.call(path, width).await
    }
}

#[cfg(test)]
mod tests {
    use crate::test::FakeResizer;
    use std::time::Duration;

    #[tokio::test]
    async fn at_most_four_resizes_run_at_once() {
        let fake = FakeResizer::new();
        let resizer = fake.resizer();
        fake.hold();
        let resizes = (0..6)
            .map(|x| {
                let resizer = resizer.clone();
                tokio::spawn(async move { resizer.resize(&format!("/fanart-{x}.jpg"), 480).await })
            })
            .collect::<Vec<_>>();
        tokio::time::timeout(Duration::from_secs(5), async {
            while fake.running() < 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::task::yield_now().await;

        let while_held = fake.running();
        fake.release();
        for resize in resizes {
            resize.await.unwrap().unwrap();
        }

        assert_eq!(while_held, 4);
        assert_eq!(fake.most_running(), 4);
        assert_eq!(fake.resized().len(), 6);
    }
}
