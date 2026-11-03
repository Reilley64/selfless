use application::ports::ExportTelemetry;
use application::ports::ExportTelemetryError;
use application::ports::MalformedTelemetry;
use application::ports::TelemetrySignal;
use bytes::Bytes;
use opentelemetry_proto::tonic::collector::logs::v1::ExportLogsServiceRequest;
use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_client::MetricsServiceClient;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_client::TraceServiceClient;
use prost::Message;
use serde::Deserialize;
use tonic::transport::Endpoint;

#[derive(Clone, Default, Deserialize)]
pub struct OtlpSettings {
    #[serde(rename = "otel_exporter_otlp_endpoint")]
    pub endpoint: Option<String>,
    #[serde(rename = "otel_exporter_otlp_traces_endpoint")]
    pub traces_endpoint: Option<String>,
    #[serde(rename = "otel_exporter_otlp_metrics_endpoint")]
    pub metrics_endpoint: Option<String>,
    #[serde(rename = "otel_exporter_otlp_logs_endpoint")]
    pub logs_endpoint: Option<String>,
}

pub fn export_telemetry(
    settings: &OtlpSettings,
) -> Result<ExportTelemetry, tonic::transport::Error> {
    let channel = |x: &Option<String>| {
        Endpoint::from_shared(
            x.as_ref()
                .or(settings.endpoint.as_ref())
                .map_or("http://localhost:4317", |x| x.as_str())
                .to_owned(),
        )
        .map(|x| x.connect_lazy())
    };
    let traces = channel(&settings.traces_endpoint)?;
    let metrics = channel(&settings.metrics_endpoint)?;
    let logs = channel(&settings.logs_endpoint)?;
    Ok(ExportTelemetry::new(move |signal, request| {
        let (traces, metrics, logs) = (traces.clone(), metrics.clone(), logs.clone());
        Box::pin(async move {
            let malformed = |_| ExportTelemetryError::caused_by(MalformedTelemetry);
            let response = match signal {
                TelemetrySignal::Traces => TraceServiceClient::new(traces)
                    .export(ExportTraceServiceRequest::decode(request).map_err(malformed)?)
                    .await
                    .map(|x| x.into_inner().encode_to_vec()),
                TelemetrySignal::Metrics => MetricsServiceClient::new(metrics)
                    .export(ExportMetricsServiceRequest::decode(request).map_err(malformed)?)
                    .await
                    .map(|x| x.into_inner().encode_to_vec()),
                TelemetrySignal::Logs => LogsServiceClient::new(logs)
                    .export(ExportLogsServiceRequest::decode(request).map_err(malformed)?)
                    .await
                    .map(|x| x.into_inner().encode_to_vec()),
            };
            response
                .map(Bytes::from)
                .map_err(ExportTelemetryError::caused_by)
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse;
    use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceService;
    use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;
    use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
    use opentelemetry_proto::tonic::trace::v1::ScopeSpans;
    use opentelemetry_proto::tonic::trace::v1::Span;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tokio::net::TcpListener;
    use tonic::Request;
    use tonic::Response;
    use tonic::Status;
    use tonic::transport::Server;
    use tonic::transport::server::TcpIncoming;

    #[tokio::test]
    async fn exported_traces_reach_the_collector() {
        let collector = FakeCollector::default();
        let settings = OtlpSettings {
            endpoint: Some(collector.start().await),
            ..Default::default()
        };
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                scope_spans: vec![ScopeSpans {
                    spans: vec![Span {
                        trace_id: vec![0x4b; 16],
                        span_id: vec![0x00; 8],
                        name: "GET".to_owned(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };

        let reply = export_telemetry(&settings)
            .unwrap()
            .call(TelemetrySignal::Traces, request.encode_to_vec().into())
            .await
            .unwrap();

        ExportTraceServiceResponse::decode(reply).unwrap();
        assert_eq!(*collector.requests.lock().unwrap(), vec![request]);
    }

    #[derive(Clone, Default)]
    struct FakeCollector {
        requests: Arc<Mutex<Vec<ExportTraceServiceRequest>>>,
    }

    impl FakeCollector {
        async fn start(&self) -> String {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            tokio::spawn(
                Server::builder()
                    .add_service(TraceServiceServer::new(self.clone()))
                    .serve_with_incoming(TcpIncoming::from(listener)),
            );
            format!("http://{address}")
        }
    }

    #[tonic::async_trait]
    impl TraceService for FakeCollector {
        async fn export(
            &self,
            request: Request<ExportTraceServiceRequest>,
        ) -> Result<Response<ExportTraceServiceResponse>, Status> {
            self.requests.lock().unwrap().push(request.into_inner());
            Ok(Response::new(ExportTraceServiceResponse::default()))
        }
    }
}
