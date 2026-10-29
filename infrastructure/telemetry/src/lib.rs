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
