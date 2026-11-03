mod inbox;
mod inbox_cleanup;
mod outbox;
mod session_cleanup;

use application::sessions::delete_expired_sessions;
use domain::Source;
use infrastructure::AppState;
use persistence::QueuePauses;
use std::error::Error;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

const QUEUE_PAUSES: QueuePauses = QueuePauses {
    idle: Duration::from_millis(100),
    after_failure: Duration::from_secs(30),
};

pub async fn start(state: &AppState) -> Result<(), delete_expired_sessions::Error> {
    session_cleanup::start(state).await
}

pub async fn serve(state: AppState, reconcile_interval: Duration, cancellation: CancellationToken) {
    tokio::join!(
        session_cleanup::serve(state.clone(), cancellation.clone()),
        outbox::serve(state.clone(), cancellation.clone()),
        inbox::serve(state.clone(), cancellation.clone()),
        inbox_cleanup::serve(state.clone(), cancellation.clone()),
        cancellation.run_until_cancelled(reconcile(state, reconcile_interval)),
    );
}

async fn reconcile(state: AppState, interval: Duration) {
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
