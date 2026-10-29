use domain::Source;
use infrastructure::AppState;
use jiff::Timestamp;
use std::error::Error;
use std::time::Duration;
use tokio::time::MissedTickBehavior;

const AFTER_AN_INBOX_ERROR: Duration = Duration::from_secs(60);
const INBOX_CLEANUP_INTERVAL: Duration = Duration::from_secs(3600);

pub async fn inbox(state: AppState) {
    let mut release = true;
    loop {
        if release {
            if let Err(error) = application::release_inbox_rows::call(&state).await {
                tracing::error!(
                    error = &error as &dyn Error,
                    "failed to release the claimed inbox rows"
                );
                tokio::time::sleep(AFTER_AN_INBOX_ERROR).await;
                continue;
            }
            release = false;
        }
        match process_due_rows(&state).await {
            Ok(Some(next_attempt_at)) => {
                let wait = Duration::try_from(next_attempt_at.duration_since(state.clock.call()))
                    .unwrap_or_default();
                tokio::select! {
                    () = state.inbox_wakeup.notified() => {}
                    () = tokio::time::sleep(wait) => {}
                }
            }
            Ok(None) => state.inbox_wakeup.notified().await,
            Err(error) => {
                tracing::error!(error = &*error as &dyn Error, "failed to process the inbox");
                release = true;
                tokio::time::sleep(AFTER_AN_INBOX_ERROR).await;
            }
        }
    }
}

async fn process_due_rows(
    state: &AppState,
) -> Result<Option<Timestamp>, Box<dyn Error + Send + Sync>> {
    while application::process_inbox_row::call(state).await? {}
    Ok(application::next_inbox_attempt::call(state).await?)
}

pub async fn inbox_cleanup(state: AppState) {
    let mut ticks = tokio::time::interval(INBOX_CLEANUP_INTERVAL);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        match application::delete_old_inbox_rows::call(&state).await {
            Ok(0) => {}
            Ok(deleted) => tracing::info!(
                selfless.webhook.deleted_deliveries = deleted,
                "deleted old inbox rows"
            ),
            Err(error) => {
                tracing::error!(
                    error = &error as &dyn Error,
                    "failed to delete old inbox rows"
                );
            }
        }
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
