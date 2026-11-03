mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::PlaybackSettings;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    autoplay_next_episode: bool,
    ask_if_still_watching: bool,
) -> Result<Output, Error> {
    let mut db_context = dependencies.into().db_context;
    let settings = PlaybackSettings::upsert_by_user_id(user_id)
        .autoplay_next_episode(autoplay_next_episode)
        .ask_if_still_watching(ask_if_still_watching)
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
    use domain::EmailAddress;

    #[tokio::test]
    async fn changing_the_settings_keeps_the_remembered_languages() {
        let mut db_context = mock_db_context().await;
        let noor = db_context
            .users()
            .create()
            .name("Noor")
            .email(EmailAddress::new("noor@example.com".to_owned()).unwrap())
            .password_hash("hash")
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        db_context
            .playback_settings()
            .create()
            .user_id(noor)
            .audio_language(Some("jpn".to_owned()))
            .subtitle_language(Some("eng".to_owned()))
            .exec(db_context.executor_mut())
            .await
            .unwrap();

        call(
            Dependencies {
                db_context: db_context.clone(),
            },
            noor,
            false,
            true,
        )
        .await
        .unwrap();

        let settings = db_context
            .playback_settings()
            .find(noor)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(settings.audio_language.as_deref(), Some("jpn"));
        assert_eq!(settings.subtitle_language.as_deref(), Some("eng"));
    }
}
