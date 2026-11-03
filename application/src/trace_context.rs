use opentelemetry::Context;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::SpanContext;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::trace::TraceFlags;
use opentelemetry::trace::TraceState;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::IdGenerator;
use opentelemetry_sdk::trace::RandomIdGenerator;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

pub fn current_traceparent() -> Option<String> {
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&Span::current().context(), &mut carrier);
    carrier.remove("traceparent")
}

pub fn unsampled_parent() -> Context {
    let ids = RandomIdGenerator::default();
    Context::new().with_remote_span_context(SpanContext::new(
        ids.new_trace_id(),
        ids.new_span_id(),
        TraceFlags::NOT_SAMPLED,
        true,
        TraceState::NONE,
    ))
}
