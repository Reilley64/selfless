use crate::attributes::selfless_auth_client_kind;
use crate::attributes::selfless_library_source;
use crate::attributes::selfless_playback_method;
use crate::attributes::selfless_reconcile_result;
use crate::attributes::selfless_watch_progress_outcome;
use crate::attributes::selfless_webhook_source;
use domain::ClientKind;
use domain::PlayMethod;
use domain::PlayReason;
use domain::ReconcileResult;
use domain::ReportOutcome;
use domain::Source;
use opentelemetry::Array;
use opentelemetry::Value;

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

pub(crate) fn client_kind(client_kind: ClientKind) -> &'static str {
    match client_kind {
        ClientKind::Web => selfless_auth_client_kind::WEB,
        ClientKind::AppleTv => selfless_auth_client_kind::APPLE_TV,
        ClientKind::GoogleTv => selfless_auth_client_kind::GOOGLE_TV,
    }
}

pub(crate) fn play_method(method: PlayMethod) -> &'static str {
    match method {
        PlayMethod::DirectPlay => selfless_playback_method::DIRECT_PLAY,
        PlayMethod::Remux => selfless_playback_method::REMUX,
        PlayMethod::Transcode => selfless_playback_method::TRANSCODE,
    }
}

pub(crate) fn reason_kinds(reasons: &[PlayReason]) -> Value {
    Value::Array(Array::String(
        reasons
            .iter()
            .map(|x| {
                match x {
                    PlayReason::Container(_) => "container",
                    PlayReason::VideoCodec(_) => "videoCodec",
                    PlayReason::VideoProfile(_) => "videoProfile",
                    PlayReason::VideoSize { .. } => "videoSize",
                    PlayReason::VideoRange(_) => "videoRange",
                    PlayReason::AudioCodec(_) => "audioCodec",
                    PlayReason::AudioChannels(_) => "audioChannels",
                    PlayReason::Subtitle(_) => "subtitle",
                    PlayReason::NoKeyframeIndex => "noKeyframeIndex",
                    PlayReason::ForcedHls => "forcedHls",
                }
                .into()
            })
            .collect(),
    ))
}

pub(crate) fn watch_progress_outcome(outcome: ReportOutcome) -> &'static str {
    match outcome {
        ReportOutcome::Nothing => selfless_watch_progress_outcome::NONE,
        ReportOutcome::ResumePosition => selfless_watch_progress_outcome::RESUME_POSITION,
        ReportOutcome::Watched => selfless_watch_progress_outcome::WATCHED,
    }
}
