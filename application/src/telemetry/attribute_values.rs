use crate::attributes::selfless_library_source;
use crate::attributes::selfless_reconcile_result;
use crate::attributes::selfless_webhook_source;
use domain::ReconcileResult;
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
