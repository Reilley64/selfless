use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::SpanContext;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const TRACEPARENT: &str = "traceparent";

pub fn current_traceparent() -> Option<String> {
    let mut carrier = HashMap::new();
    TraceContextPropagator::new().inject_context(&Span::current().context(), &mut carrier);
    carrier.remove(TRACEPARENT)
}

pub fn span_context(traceparent: &str) -> SpanContext {
    let carrier = HashMap::from([(TRACEPARENT.to_owned(), traceparent.to_owned())]);
    TraceContextPropagator::new()
        .extract(&carrier)
        .span()
        .span_context()
        .clone()
}
