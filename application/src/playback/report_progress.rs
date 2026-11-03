mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Playback;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    user_id: Uuid,
    playback_id: Uuid,
    position: f64,
    paused: bool,
    stopped: bool,
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
    let now = clock.call();
    playback
        .update()
        .position(position)
        .paused(paused)
        .last_reported_at(now)
        .ended_at(stopped.then_some(now))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    if stopped {
        end_hls_session.call(playback_id).await;
    }
    Ok(Output::Reported)
}
