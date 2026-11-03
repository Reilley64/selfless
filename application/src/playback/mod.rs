pub mod change_playback_settings;
pub mod get_playback_file;
pub mod get_playback_settings;
pub mod report_progress;
pub mod start_playback;

use uuid::Uuid;

fn resource(playback_id: Uuid) -> String {
    format!("playbacks/{playback_id}")
}
