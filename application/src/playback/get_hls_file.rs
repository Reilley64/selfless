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
    name: &str,
) -> Result<Output, Error> {
    let Dependencies {
        mut db_context,
        url_signer,
        get_hls_file,
    } = dependencies.into();
    if !url_signer.verify_signature(&resource(playback_id), signature) {
        return Ok(Output::InvalidSignature);
    }
    let open = db_context
        .playbacks()
        .find(playback_id)
        .exec(db_context.executor_mut())
        .await
        .map_err(Error::caused_by)?
        .is_some_and(|x| !x.ended());
    if !open {
        return Ok(Output::NotFound);
    }
    let path = get_hls_file
        .call(playback_id, name)
        .await
        .map_err(Error::caused_by)?;
    Ok(path.map_or(Output::NotFound, |path| Output::File { path }))
}
