use infrastructure::AppState;
use std::error::Error;

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
