use domain::Source;
use infrastructure::AppState;
use std::error::Error;
use std::time::Duration;
use tokio::time::MissedTickBehavior;

pub async fn inbox(state: AppState) {
    if let Err(error) = application::release_inbox_rows::call(&state).await {
        tracing::error!(
            error = &error as &dyn Error,
            "failed to release the claimed inbox rows"
        );
    }
    loop {
        loop {
            match application::process_inbox_row::call(&state).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(error) => {
                    tracing::error!(
                        error = &error as &dyn Error,
                        "failed to process an inbox row"
                    );
                    break;
                }
            }
        }
        state.inbox_wakeup.notified().await;
    }
}

pub async fn reconcile(state: AppState, interval: Duration) {
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {}
            () = state.reconciles.requested() => {}
        }
        for source in [Source::Radarr, Source::Sonarr] {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = application::reconcile::call(&state, source).await {
                    tracing::error!(error = &error as &dyn Error, "failed to record a reconcile");
                }
            });
        }
    }
}
