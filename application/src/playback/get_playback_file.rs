mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::playback::resource;
use crate::ports::persistence::DbScope;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    playback_id: Uuid,
    signature: &str,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        url_signer,
    } = dependencies.into();
    if !url_signer.verify_signature(&resource(playback_id), signature) {
        return Ok(Output::InvalidSignature);
    }
    let Some(playback) = db_context
        .playbacks()
        .find(playback_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .filter(|x| !x.ended())
    else {
        return Ok(Output::NotFound);
    };
    let media_file = db_context
        .media_files()
        .find(playback.media_file_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?;
    Ok(media_file.map_or(Output::NotFound, |x| Output::File { path: x.path }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::persistence::DbContext;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;
    use crate::test::url_signer;

    const NOW: &str = "2026-11-03T20:00:00Z";
    const PATH: &str = "/media/Movies/Arrival (2016)/Arrival (2016) Bluray-1080p.mp4";

    async fn file(db_context: &DbContext, playback_id: Uuid, signature: &str) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
                url_signer: url_signer(),
            },
            playback_id,
            signature,
        )
        .await
        .unwrap()
    }

    async fn add_playback(db_context: &mut DbContext, ended: bool) -> Uuid {
        let media_file_id = db_context
            .media_files()
            .create()
            .path(PATH)
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;
        playback(db_context, at(NOW))
            .media_file_id(media_file_id)
            .ended_at(ended.then(|| at(NOW)))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn a_signed_url_of_an_open_playback_serves_its_media_file() {
        let mut db_context = mock_db_context().await;
        let playback_id = add_playback(&mut db_context, false).await;

        let output = file(
            &db_context,
            playback_id,
            &url_signer().signature(&resource(playback_id)),
        )
        .await;

        assert!(
            matches!(&output, Output::File { path } if path == PATH),
            "{output:?}"
        );
    }

    #[tokio::test]
    async fn a_signature_for_another_playback_is_invalid() {
        let mut db_context = mock_db_context().await;
        let playback_id = add_playback(&mut db_context, false).await;
        let other_id = playback(&db_context, at(NOW))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id;

        let output = file(
            &db_context,
            playback_id,
            &url_signer().signature(&resource(other_id)),
        )
        .await;

        assert!(matches!(output, Output::InvalidSignature), "{output:?}");
    }

    #[tokio::test]
    async fn the_url_of_an_ended_playback_is_not_found() {
        let mut db_context = mock_db_context().await;
        let playback_id = add_playback(&mut db_context, true).await;

        let output = file(
            &db_context,
            playback_id,
            &url_signer().signature(&resource(playback_id)),
        )
        .await;

        assert!(matches!(output, Output::NotFound), "{output:?}");
    }
}
