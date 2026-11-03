use application::playback::end_silent_playbacks;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(10);

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut ticks = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(ticks.tick())
        .await
        .is_some()
    {
        if let Err(error) = end_silent_playbacks::call(&state).await {
            tracing::error!(
                error = &error as &dyn Error,
                "failed to end silent playbacks"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::persistence::DbScope;
    use domain::PlayMethod;
    use jiff::SignedDuration;
    use test_app::TestApp;
    use uuid::Uuid;

    async fn open_playbacks(app: &TestApp) -> usize {
        let mut db_context = app.state.db_context.clone();
        db_context
            .playbacks()
            .filter(domain::Playback::open_filter())
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .len()
    }

    #[tokio::test(start_paused = true)]
    async fn serving_ends_a_silent_playback_within_10_seconds() {
        let app = TestApp::new().await;
        let mut db_context = app.state.db_context.clone();
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
            .started_at(app.clock.now())
            .last_reported_at(app.clock.now())
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        app.clock.advance(SignedDuration::from_secs(60));
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), cancellation.clone()));
        tokio::task::yield_now().await;

        tokio::time::advance(PERIOD - Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let before_the_period = open_playbacks(&app).await;
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        let after_the_period = open_playbacks(&app).await;
        cancellation.cancel();
        serving.await.unwrap();

        assert_eq!(before_the_period, 1);
        assert_eq!(after_the_period, 0);
    }
}
