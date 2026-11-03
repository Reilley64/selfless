use application::library::delete_old_deliveries;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const PERIOD: Duration = Duration::from_secs(60 * 60);

pub(super) async fn serve(state: AppState, cancellation: CancellationToken) {
    let mut hourly = tokio::time::interval_at(Instant::now() + PERIOD, PERIOD);
    while cancellation
        .run_until_cancelled(hourly.tick())
        .await
        .is_some()
    {
        match delete_old_deliveries::call(&state).await {
            Ok(output) if output.deleted > 0 => tracing::info!(
                selfless.webhook.deleted_deliveries = output.deleted,
                "deleted old webhook deliveries"
            ),
            Ok(_) => {}
            Err(error) => tracing::error!(
                error = &error as &dyn Error,
                "failed to delete old webhook deliveries"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::persistence::DbScope;
    use domain::Source;
    use jiff::SignedDuration;
    use test_app::TestApp;

    async fn bodies(app: &TestApp) -> Vec<Vec<u8>> {
        let mut db_context = app.state.db_context.clone();
        db_context
            .inboxes()
            .all()
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .into_iter()
            .map(|x| x.body)
            .collect()
    }

    async fn let_serve_run() {
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn serving_deletes_old_deliveries_an_hour_after_start() {
        let app = TestApp::new().await;
        let mut db_context = app.state.db_context.clone();
        db_context
            .inboxes()
            .create()
            .source(Source::Radarr)
            .body(b"old".to_vec())
            .received_at(app.clock.now())
            .delivered_at(Some(app.clock.now()))
            .exec(db_context.executor_mut())
            .await
            .unwrap();
        app.clock.advance(SignedDuration::from_hours(30 * 24));
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), cancellation.clone()));
        let_serve_run().await;

        tokio::time::advance(PERIOD - Duration::from_secs(1)).await;
        let_serve_run().await;
        let before_an_hour = bodies(&app).await;
        tokio::time::advance(Duration::from_secs(1)).await;
        let_serve_run().await;
        cancellation.cancel();
        serving.await.unwrap();
        let after_an_hour = bodies(&app).await;

        assert_eq!(before_an_hour, [b"old".to_vec()]);
        assert!(after_an_hour.is_empty());
    }
}
