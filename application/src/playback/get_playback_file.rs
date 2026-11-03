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
