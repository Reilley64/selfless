mod dependencies;
mod error;
mod output;

pub use dependencies::Dependencies;
pub use error::Error;
pub use output::Output;

use crate::playback::SignedPlayback;
use crate::playback::signed_playback;
use uuid::Uuid;

pub async fn call(
    dependencies: impl Into<Dependencies>,
    playback_id: Uuid,
    signature: &str,
    name: &str,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        url_signer,
        get_hls_file,
    } = dependencies.into();
    match signed_playback(&mut db_context, &url_signer, playback_id, signature)
        .await
        .map_err(Error::caused_by)?
    {
        SignedPlayback::Open(_) => {}
        SignedPlayback::NotFound => return Ok(Output::NotFound),
        SignedPlayback::InvalidSignature => return Ok(Output::InvalidSignature),
    }
    let path = get_hls_file
        .call(playback_id, name)
        .await
        .map_err(Error::caused_by)?;
    Ok(path.map_or(Output::NotFound, |path| Output::File { path }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::resource;
    use crate::ports::GetHlsFile;
    use crate::ports::persistence::DbContext;
    use crate::ports::persistence::DbScope;
    use crate::test::at;
    use crate::test::mock_db_context;
    use crate::test::playback;
    use crate::test::url_signer;
    use std::sync::Arc;
    use std::sync::Mutex;

    const NOW: &str = "2026-11-03T20:00:00Z";

    #[derive(Clone, Default)]
    struct FakeHlsFiles {
        requested: Arc<Mutex<Vec<(Uuid, String)>>>,
    }

    impl FakeHlsFiles {
        fn port(&self) -> GetHlsFile {
            let requested = self.requested.clone();
            GetHlsFile::new(move |playback_id, name| {
                let path = (name == "main.m3u8").then(|| format!("/segments/{playback_id}/{name}"));
                requested.lock().unwrap().push((playback_id, name));
                Box::pin(async move { Ok(path) })
            })
        }

        fn requested(&self) -> Vec<(Uuid, String)> {
            self.requested.lock().unwrap().clone()
        }
    }

    async fn hls_file(
        db_context: &DbContext,
        files: &FakeHlsFiles,
        playback_id: Uuid,
        signature: &str,
        name: &str,
    ) -> Output {
        call(
            Dependencies {
                db_context: db_context.clone(),
                url_signer: url_signer(),
                get_hls_file: files.port(),
            },
            playback_id,
            signature,
            name,
        )
        .await
        .unwrap()
    }

    async fn add_playback(db_context: &mut DbContext, ended: bool) -> Uuid {
        playback(db_context, at(NOW))
            .ended_at(ended.then(|| at(NOW)))
            .exec(db_context.executor_mut())
            .await
            .unwrap()
            .id
    }

    fn signature(playback_id: Uuid) -> String {
        url_signer().signature(&resource(playback_id))
    }

    #[tokio::test]
    async fn a_signed_url_of_an_open_playback_serves_the_file_its_hls_output_has() {
        let mut db_context = mock_db_context().await;
        let playback_id = add_playback(&mut db_context, false).await;
        let files = FakeHlsFiles::default();

        let main = hls_file(
            &db_context,
            &files,
            playback_id,
            &signature(playback_id),
            "main.m3u8",
        )
        .await;
        let missing = hls_file(
            &db_context,
            &files,
            playback_id,
            &signature(playback_id),
            "9.m4s",
        )
        .await;

        let expected = format!("/segments/{playback_id}/main.m3u8");
        assert!(
            matches!(&main, Output::File { path } if *path == expected),
            "{main:?}"
        );
        assert!(matches!(missing, Output::NotFound), "{missing:?}");
    }

    #[tokio::test]
    async fn a_bad_signature_or_an_ended_playback_never_reaches_the_hls_output() {
        let mut db_context = mock_db_context().await;
        let open = add_playback(&mut db_context, false).await;
        let ended = add_playback(&mut db_context, true).await;
        let files = FakeHlsFiles::default();

        let bad_signature =
            hls_file(&db_context, &files, open, &signature(ended), "main.m3u8").await;
        let after_the_end =
            hls_file(&db_context, &files, ended, &signature(ended), "main.m3u8").await;

        assert!(
            matches!(bad_signature, Output::InvalidSignature),
            "{bad_signature:?}"
        );
        assert!(
            matches!(after_the_end, Output::NotFound),
            "{after_the_end:?}"
        );
        assert_eq!(files.requested(), []);
    }
}
