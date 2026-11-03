mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::MediaFile;
use domain::Playback;
use domain::PlaybackSettings;
use domain::SubtitleTrack;
use domain::TrackLanguage;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    playback_id: Uuid,
    position: f64,
    paused: bool,
    stopped: bool,
    audio_track_id: Option<Uuid>,
    subtitle_track_id: Option<Uuid>,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        end_hls_session,
    } = dependencies.into();
    let Some(mut playback) = db_context
        .playbacks()
        .filter(
            Playback::fields()
                .id()
                .eq(playback_id)
                .and(Playback::fields().user_id().eq(user_id)),
        )
        .first()
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
    else {
        return Ok(Output::NotFound);
    };
    if playback.ended() {
        return Ok(Output::Ended);
    }
    let audio_changed = playback.audio_track_id != audio_track_id;
    let subtitle_changed = playback.subtitle_track_id != subtitle_track_id;
    let now = clock.call();
    let mut transaction = db_context.transaction().await.map_err(Error::caused_by)?;
    if audio_changed || subtitle_changed {
        let media_file = transaction
            .media_files()
            .find(playback.media_file_id)
            .include(MediaFile::fields().audio_streams())
            .include(MediaFile::fields().subtitle_streams())
            .include(MediaFile::fields().subtitle_files())
            .exec(transaction.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        let audio_language = media_file
            .iter()
            .flat_map(|x| x.audio_streams.get())
            .find(|x| audio_changed && Some(x.id) == audio_track_id)
            .and_then(|x| x.language.as_deref())
            .map(TrackLanguage::remembered);
        let subtitle_language = match subtitle_track_id {
            _ if !subtitle_changed => None,
            None => Some(None),
            Some(id) => media_file
                .as_ref()
                .and_then(|x| {
                    SubtitleTrack::all(x.subtitle_streams.get(), x.subtitle_files.get())
                        .into_iter()
                        .find(|x| x.id() == id)
                })
                .and_then(|x| x.language())
                .map(|x| Some(TrackLanguage::remembered(x))),
        };
        if audio_language.is_some() || subtitle_language.is_some() {
            let mut settings = PlaybackSettings::upsert_by_user_id(user_id);
            if let Some(language) = audio_language {
                settings = settings.audio_language(Some(language));
            }
            if let Some(language) = subtitle_language {
                settings = settings.subtitle_language(language);
            }
            settings
                .exec(transaction.executor_mut())
                .await
                .map_err(Error::caused_by)?;
        }
    }
    playback
        .update()
        .position(position)
        .paused(paused)
        .audio_track_id(audio_track_id)
        .subtitle_track_id(subtitle_track_id)
        .last_reported_at(now)
        .ended_at(stopped.then_some(now))
        .exec(transaction.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    transaction.commit().await.map_err(Error::caused_by)?;
    if stopped {
        end_hls_session.call(playback_id).await;
    }
    Ok(Output::Reported)
}
