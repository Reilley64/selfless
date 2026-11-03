use application::telemetry::attribute_values::library_source;
use domain::Source;
use infrastructure::AppState;
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

pub(super) async fn serve(state: AppState, interval: Duration, cancellation: CancellationToken) {
    cancellation
        .run_until_cancelled(reconcile(state, interval))
        .await;
}

async fn reconcile(state: AppState, interval: Duration) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut running = JoinSet::new();
    let mut sources = HashMap::new();
    loop {
        tokio::select! {
            _ = ticks.tick() => {}
            () = state.reconciles.requested() => {}
            Some(joined) = running.join_next_with_id() => {
                match joined {
                    Ok((id, ())) => {
                        sources.remove(&id);
                    }
                    Err(error) => {
                        let source = sources.remove(&error.id()).map(library_source);
                        tracing::error!(
                            error = &error as &dyn Error,
                            selfless.library.source = source,
                            "a reconcile panicked"
                        );
                    }
                }
                continue;
            }
        }
        for source in [Source::Radarr, Source::Sonarr] {
            let state = state.clone();
            let task = running.spawn(async move {
                if let Err(error) = application::library::reconcile::call(&state, source).await {
                    tracing::error!(error = &error as &dyn Error, "failed to record a reconcile");
                }
            });
            sources.insert(task.id(), source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::library::sync_now;
    use application::ports::ListRadarrMovies;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use test_app::TestApp;
    use test_fixtures::Logs;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::layer::SubscriberExt;

    const INTERVAL: Duration = Duration::from_secs(3600);

    fn count_listings(app: &mut TestApp) -> Arc<AtomicUsize> {
        let listings = Arc::new(AtomicUsize::new(0));
        let counted = listings.clone();
        app.state.list_radarr_movies = ListRadarrMovies::new(move || {
            counted.fetch_add(1, Ordering::Relaxed);
            Box::pin(async { Ok(Vec::new()) })
        });
        listings
    }

    async fn let_serve_run() {
        tokio::task::yield_now().await;
    }

    #[tokio::test(start_paused = true)]
    async fn serving_reconciles_at_start_and_again_after_the_reconcile_interval() {
        let mut app = TestApp::new().await;
        let listings = count_listings(&mut app);
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), INTERVAL, cancellation.clone()));
        let_serve_run().await;
        let at_start = listings.load(Ordering::Relaxed);

        tokio::time::advance(INTERVAL - Duration::from_secs(1)).await;
        let_serve_run().await;
        let before_the_interval = listings.load(Ordering::Relaxed);
        tokio::time::advance(Duration::from_secs(1)).await;
        let_serve_run().await;
        let after_the_interval = listings.load(Ordering::Relaxed);
        cancellation.cancel();
        serving.await.unwrap();

        assert_eq!(at_start, 1);
        assert_eq!(before_the_interval, 1);
        assert_eq!(after_the_interval, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn sync_now_starts_a_reconcile_before_the_interval() {
        let mut app = TestApp::new().await;
        let listings = count_listings(&mut app);
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), INTERVAL, cancellation.clone()));
        let_serve_run().await;

        sync_now::call(&app.state).unwrap();
        let_serve_run().await;
        let after_sync_now = listings.load(Ordering::Relaxed);
        cancellation.cancel();
        serving.await.unwrap();

        assert_eq!(after_sync_now, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_reconcile_is_logged_with_its_source() {
        let mut app = TestApp::new().await;
        app.state.list_radarr_movies =
            ListRadarrMovies::new(|| panic!("an unloaded relation was read"));
        let logs = Logs::default();
        let writer = logs.clone();
        let _logging = test_fixtures::set_default_subscriber(
            tracing_subscriber::registry()
                .with(LevelFilter::ERROR)
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(move || writer.clone()),
                ),
        );
        let cancellation = CancellationToken::new();
        let serving = tokio::spawn(serve(app.state.clone(), INTERVAL, cancellation.clone()));

        for _ in 0..100 {
            if logs.text().contains("a reconcile panicked") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancellation.cancel();
        serving.await.unwrap();

        let logs = logs.text();
        assert_eq!(logs.matches(" ERROR ").count(), 1, "{logs}");
        assert!(logs.contains("a reconcile panicked"), "{logs}");
        assert!(logs.contains("an unloaded relation was read"), "{logs}");
        assert!(logs.contains("radarr"), "{logs}");
    }

    #[tokio::test(start_paused = true)]
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
        let serving = tokio::spawn(serve(app.state.clone(), INTERVAL, cancellation.clone()));
        let_serve_run().await;
        let while_running = Arc::strong_count(&listing);

        cancellation.cancel();
        serving.await.unwrap();
        let_serve_run().await;

        assert_eq!(while_running, 3);
        assert_eq!(Arc::strong_count(&listing), 2);
    }
}
