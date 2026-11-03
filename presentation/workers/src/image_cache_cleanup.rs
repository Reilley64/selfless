use application::browsing::clean_image_cache;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(24 * 60 * 60);

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut daily = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(daily.tick())
        .await
        .is_some()
    {
        if let Err(error) = clean_image_cache::call(&state).await {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to clean the image cache"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::CachedImage;
    use application::ports::DeleteCachedImage;
    use application::ports::ListCachedImages;
    use std::sync::Arc;
    use std::sync::Mutex;
    use test_app::TestApp;
    use uuid::Uuid;

    #[tokio::test(start_paused = true)]
    async fn serving_deletes_cached_images_of_removed_artwork_once_a_day() {
        let mut app = TestApp::new().await;
        let cached = Arc::new(Mutex::new(vec![CachedImage {
            image_id: Uuid::new_v4(),
            width: 480,
            modified_at: app.clock.now(),
        }]));
        app.state.list_cached_images = ListCachedImages::new({
            let cached = cached.clone();
            move || {
                let images = cached.lock().unwrap().clone();
                Box::pin(async move { Ok(images) })
            }
        });
        app.state.delete_cached_image = DeleteCachedImage::new({
            let cached = cached.clone();
            move |image| {
                cached.lock().unwrap().retain(|x| *x != image);
                Box::pin(async { Ok(()) })
            }
        });
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), cancellation.clone()));
        tokio::task::yield_now().await;

        tokio::time::advance(PERIOD - Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let before_a_day = cached.lock().unwrap().len();
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let after_a_day = cached.lock().unwrap().len();
        cancellation.cancel();
        serving.await.unwrap();

        assert_eq!(before_a_day, 1);
        assert_eq!(after_a_day, 0);
    }
}
