pub mod change_playback_settings;
pub mod delete_old_playbacks;
pub mod end_silent_playbacks;
pub mod get_playback_file;
pub mod get_playback_settings;
pub mod record_active_playbacks;
pub mod report_progress;
pub mod start_playback;

use uuid::Uuid;

fn resource(playback_id: Uuid) -> String {
    format!("playbacks/{playback_id}")
}
