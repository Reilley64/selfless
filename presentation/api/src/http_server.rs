use opentelemetry::Context;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::Histogram;
use opentelemetry::propagation::Extractor;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use std::time::Instant;
use topcoat::context::Cx;
use topcoat::context::try_request_context;
use topcoat::router::Body;
use topcoat::router::HeaderMap;
use topcoat::router::Layer;
use topcoat::router::LayerFuture;
use topcoat::router::Next;
use topcoat::router::Path;
use topcoat::router::error::RewriteError;
use topcoat::router::header::USER_AGENT;
use topcoat::router::request::client_ip;
use topcoat::router::request::headers;
use topcoat::router::request::method;
use topcoat::router::request::uri;
use topcoat::router::request::version;
use topcoat::router::response::IntoResponse;
use topcoat::router::try_endpoint;
use tracing::Instrument;
use tracing::Span;
use tracing::field;
use tracing_opentelemetry::OpenTelemetrySpanExt;

const OTHER_METHOD: &str = "_OTHER";

pub(crate) struct HttpServerLayer {
    duration: Histogram<f64>,
}

impl HttpServerLayer {
    pub(crate) fn new() -> Self {
        Self {
            duration: global::meter("selfless")
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .with_boundaries(vec![
                    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
                ])
                .build(),
        }
    }
}

impl Layer for HttpServerLayer {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        let rewritten = try_request_context::<Rewritten>(cx);
        let start = rewritten.map_or_else(Instant::now, |x| x.start);
        let request_method = match method(cx).as_str() {
            x @ ("CONNECT" | "DELETE" | "GET" | "HEAD" | "OPTIONS" | "PATCH" | "POST" | "PUT"
            | "QUERY" | "TRACE") => x,
            _ => OTHER_METHOD,
        };
        let route = rewritten.map_or_else(
            || {
                try_endpoint(cx).map(|x| match x.path().as_str() {
                    "" => "/".to_owned(),
                    x => x.to_owned(),
                })
            },
            |x| x.route.clone(),
        );
        let scheme = uri(cx).scheme_str().unwrap_or("http");
        let protocol_version = match *version(cx) {
            http::Version::HTTP_09 => Some("0.9"),
            http::Version::HTTP_10 => Some("1.0"),
            http::Version::HTTP_11 => Some("1.1"),
            http::Version::HTTP_2 => Some("2"),
            http::Version::HTTP_3 => Some("3"),
            _ => None,
        };
        let parent = TraceContextPropagator::new()
            .extract_with_context(&Context::new(), &Traceparent(headers(cx)));
        let span = if let Some(rewritten) = rewritten {
            rewritten.span.clone()
        } else if uri(cx).path().starts_with("/actuator/health")
            && !parent.span().span_context().is_sampled()
        {
            Span::none()
        } else {
            let span_method = if request_method == OTHER_METHOD {
                "HTTP"
            } else {
                request_method
            };
            let span = tracing::info_span!(
                parent: None,
                "http_request",
                otel.name = %route.as_ref().map_or_else(|| span_method.to_owned(), |x| format!("{span_method} {x}")),
                otel.kind = "server",
                otel.status_code = field::Empty,
                http.request.method = request_method,
                http.request.method_original = (request_method == OTHER_METHOD).then(|| method(cx).as_str()),
                http.route = route,
                http.response.status_code = field::Empty,
                url.path = uri(cx).path(),
                url.query = uri(cx).query(),
                url.scheme = scheme,
                network.protocol.version = protocol_version,
                client.address = client_ip(cx).map(field::display),
                user_agent.original = headers(cx).get(USER_AGENT).and_then(|x| x.to_str().ok()),
                error.type = field::Empty,
            );
            let _ = span.set_parent(parent);
            span
        };
        let recorder = span.clone();
        Box::pin(
            async move {
                let response = match next.run(cx, body).await {
                    Ok(response) => response,
                    Err(error) => match error.downcast::<RewriteError>() {
                        Ok(rewrite) => {
                            return Err(rewrite
                                .with(Rewritten {
                                    span: recorder,
                                    start,
                                    route,
                                })
                                .into());
                        }
                        Err(error) => error.into_error().into_response(cx)?,
                    },
                };
                let status = response.status();
                recorder.record("http.response.status_code", status.as_u16());
                let mut attributes = vec![
                    KeyValue::new("http.request.method", request_method.to_owned()),
                    KeyValue::new("url.scheme", scheme.to_owned()),
                    KeyValue::new("http.response.status_code", i64::from(status.as_u16())),
                ];
                if let Some(route) = route {
                    attributes.push(KeyValue::new("http.route", route));
                }
                if let Some(protocol_version) = protocol_version {
                    attributes.push(KeyValue::new("network.protocol.version", protocol_version));
                }
                if status.is_server_error() {
                    recorder.record("otel.status_code", "ERROR");
                    recorder.record("error.type", status.as_str());
                    attributes.push(KeyValue::new("error.type", status.as_str().to_owned()));
                }
                self.duration
                    .record(start.elapsed().as_secs_f64(), &attributes);
                Ok(response)
            }
            .instrument(span),
        )
    }
}

struct Rewritten {
    span: Span,
    start: Instant,
    route: Option<String>,
}

struct Traceparent<'a>(&'a HeaderMap);

impl Extractor for Traceparent<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        match key {
            "traceparent" => self.0.get(key)?.to_str().ok(),
            _ => None,
        }
    }

    fn keys(&self) -> Vec<&str> {
        vec!["traceparent"]
    }
}
