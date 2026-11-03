mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::ports::persistence::DbScope;
use domain::Playback;

pub async fn call(dependencies: impl Into<Dependencies>) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        clock,
        end_hls_session,
    } = dependencies.into();
    let now = clock.call();
    let silent = db_context
        .playbacks()
        .filter(Playback::silent_filter(now))
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    for mut playback in silent {
        playback
            .update()
            .ended_at(Some(now))
            .exec(db_context.executor_mut())
            .await
            .map_err(Error::caused_by)?;
        end_hls_session.call(playback.id).await;
    }
    Ok(Output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::ports::persistence::DbContext;
    use crate::test::EndedHlsSessions;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;
    use jiff::Timestamp;
    use uuid::Uuid;

    const NOW: &str = "2026-06-01T12:00:00Z";

    async fn end_silent_playbacks(db_context: &DbContext) -> Vec<Uuid> {
        let ended = EndedHlsSessions::default();
        call(Dependencies {
            db_context: db_context.clone(),
            clock: Clock::new(|| at(NOW)),
            end_hls_session: ended.port(),
        })
        .await
        .unwrap();
        ended.playback_ids()
    }

    async fn ended_at(db_context: &mut DbContext, id: Uuid) -> Option<Timestamp> {
        db_context
            .playbacks()
            .find(id)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .unwrap()
            .ended_at
    }

    #[tokio::test]
    async fn a_playback_with_no_report_for_60_seconds_ends_with_its_hls_session() {
        let mut db_context = mock_db_context().await;
        let silent = playback(&db_context, at("2026-06-01T11:59:00Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        let reporting = playback(&db_context, at("2026-06-01T11:59:01Z"))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        let ended_hls_sessions = end_silent_playbacks(&db_context).await;

        assert_eq!(ended_at(&mut db_context, silent).await, Some(at(NOW)));
        assert_eq!(ended_at(&mut db_context, reporting).await, None);
        assert_eq!(ended_hls_sessions, [silent]);
    }

    #[tokio::test]
    async fn a_playback_that_already_ended_keeps_its_end_time() {
        let mut db_context = mock_db_context().await;
        let stopped_at = at("2026-06-01T11:55:00Z");
        let stopped = playback(&db_context, stopped_at)
            .ended_at(Some(stopped_at))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        let ended_hls_sessions = end_silent_playbacks(&db_context).await;

        assert_eq!(ended_at(&mut db_context, stopped).await, Some(stopped_at));
        assert_eq!(ended_hls_sessions, Vec::<Uuid>::new());
    }
}
