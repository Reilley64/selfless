use opentelemetry::Context;
use opentelemetry::trace::SpanContext;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::trace::TraceFlags;
use opentelemetry::trace::TraceState;
use opentelemetry_sdk::trace::IdGenerator;
use opentelemetry_sdk::trace::RandomIdGenerator;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub(crate) fn inbox_poll() -> Span {
    let span = tracing::info_span!(parent: None, "inbox_poll");
    let ids = RandomIdGenerator::default();
    let _ = span.set_parent(Context::new().with_remote_span_context(SpanContext::new(
        ids.new_trace_id(),
        ids.new_span_id(),
        TraceFlags::NOT_SAMPLED,
        true,
        TraceState::NONE,
    )));
    span
}
