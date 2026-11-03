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
