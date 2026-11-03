mod image_cache_cleanup;
mod inbox;
mod inbox_cleanup;
mod outbox;
mod session_cleanup;

use application::sessions::delete_expired_sessions;
use domain::Source;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

pub async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    session_cleanup::start(state).await
}

pub async fn serve(state: AppState, reconcile_interval: Duration, cancellation: CancellationToken) {
    tokio::join!(
        session_cleanup::serve(state.clone(), cancellation.clone()),
        outbox::serve(state.clone(), cancellation.clone()),
        inbox::serve(state.clone(), cancellation.clone()),
        inbox_cleanup::serve(state.clone(), cancellation.clone()),
        image_cache_cleanup::serve(state.clone(), cancellation.clone()),
        cancellation.run_until_cancelled(reconcile(state, reconcile_interval)),
    );
}

async fn reconcile(state: AppState, interval: Duration) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut running = JoinSet::new();
    loop {
        tokio::select! {
            _ = ticks.tick() => {}
            () = state.reconciles.requested() => {}
        }
        while running.try_join_next().is_some() {}
        for source in [Source::Radarr, Source::Sonarr] {
            let state = state.clone();
            running.spawn(async move {
                if let Err(error) = application::library::reconcile::call(&state, source).await {
                    tracing::error!(error = &error as &dyn Error, "failed to record a reconcile");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::ListRadarrMovies;
    use std::sync::Arc;
    use test_app::TestApp;

    async fn until(condition: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn shutdown_stops_a_running_reconcile() {
        let mut app = TestApp::new().await;
        let listing = Arc::new(());
        let port_listing = listing.clone();
        app.state.list_radarr_movies = ListRadarrMovies::new(move || {
            let listing = port_listing.clone();
            Box::pin(async move {
                let _listing = listing;
                std::future::pending().await
            })
        });
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(
            app.state.clone(),
            Duration::from_secs(3600),
            cancellation.clone(),
        ));
        until(|| Arc::strong_count(&listing) == 3).await;

        cancellation.cancel();
        serving.await.unwrap();

        until(|| Arc::strong_count(&listing) == 2).await;
    }
}
