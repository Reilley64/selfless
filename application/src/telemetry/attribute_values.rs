use crate::attributes::selfless_library_source;
use crate::attributes::selfless_reconcile_result;
use crate::attributes::selfless_watch_progress_outcome;
use crate::attributes::selfless_webhook_source;
use domain::ReconcileResult;
use domain::ReportOutcome;
use domain::Source;

pub fn library_source(source: Source) -> &'static str {
    match source {
        Source::Radarr => selfless_library_source::RADARR,
        Source::Sonarr => selfless_library_source::SONARR,
    }
}

pub(crate) fn webhook_source(source: Source) -> &'static str {
    match source {
        Source::Radarr => selfless_webhook_source::RADARR,
        Source::Sonarr => selfless_webhook_source::SONARR,
    }
}

pub(crate) fn reconcile_result(result: ReconcileResult) -> &'static str {
    match result {
        ReconcileResult::Succeeded => selfless_reconcile_result::SUCCEEDED,
        ReconcileResult::Failed => selfless_reconcile_result::FAILED,
    }
}

pub(crate) fn watch_progress_outcome(outcome: ReportOutcome) -> &'static str {
    match outcome {
        ReportOutcome::Nothing => selfless_watch_progress_outcome::NONE,
        ReportOutcome::ResumePosition => selfless_watch_progress_outcome::RESUME_POSITION,
        ReportOutcome::Watched => selfless_watch_progress_outcome::WATCHED,
    }
}
