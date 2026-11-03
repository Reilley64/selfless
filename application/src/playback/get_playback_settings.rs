mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::PlaybackSettings;
use uuid::Uuid;

pub async fn call(dependencies: impl Into<Dependencies>, user_id: Uuid) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    PlaybackSettings::upsert_by_user_id(user_id)
        .or_ignore()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    let settings = db_context
        .playback_settings()
        .filter(PlaybackSettings::fields().user_id().eq(user_id))
        .one()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(Output {
        autoplay_next_episode: settings.autoplay_next_episode,
        ask_if_still_watching: settings.ask_if_still_watching,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::mock_db_context;

    #[tokio::test]
    async fn playback_settings_start_with_autoplay_on_and_the_still_watching_prompt_off() {
        let db_context = mock_db_context().await;

        let settings = call(Dependencies { db_context }, Uuid::new_v4())
            .await
            .unwrap();

        assert_eq!(
            (
                settings.autoplay_next_episode,
                settings.ask_if_still_watching
            ),
            (true, false)
        );
    }
}
