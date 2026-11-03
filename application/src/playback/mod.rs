pub mod change_playback_settings;
pub mod delete_old_playbacks;
pub mod end_open_playbacks;
pub mod end_silent_playbacks;
pub mod get_hls_file;
pub mod get_playback_file;
pub mod get_playback_settings;
pub mod record_active_playbacks;
pub mod report_progress;
pub mod start_playback;

use crate::ports::persistence::DbScope;
use crate::url_signer::UrlSigner;
use domain::Playback;
use uuid::Uuid;

fn resource(playback_id: Uuid) -> String {
    format!("playbacks/{playback_id}")
}

enum SignedPlayback {
    Open(Playback),
    NotFound,
    InvalidSignature,
}

async fn signed_playback(
    db_context: &mut impl DbScope,
    url_signer: &UrlSigner,
    playback_id: Uuid,
    signature: &str,
) -> toasty::Result<SignedPlayback> {
    if !url_signer.verify_signature(&resource(playback_id), signature) {
        return Ok(SignedPlayback::InvalidSignature);
    }
    let playback = db_context
        .playbacks()
        .find(playback_id)
        .exec(db_context.executor_mut())
        .await?
        .filter(|x| !x.ended());
    Ok(playback.map_or(SignedPlayback::NotFound, SignedPlayback::Open))
}
