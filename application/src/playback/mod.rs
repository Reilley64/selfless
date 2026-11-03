pub mod get_playback_file;
pub mod report_progress;
pub mod start_playback;

use uuid::Uuid;

fn resource(playback_id: Uuid) -> String {
    format!("playbacks/{playback_id}")
}
