use ohno::AppError;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::trace::TracerProvider;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::LogExporter;
use opentelemetry_otlp::MetricExporter;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::error::Error;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use uuid::Uuid;

pub(crate) struct Telemetry {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
    logger_provider: SdkLoggerProvider,
}

pub(crate) fn init() -> Result<Telemetry, AppError> {
    let resource = Resource::builder()
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("service.instance.id", Uuid::new_v4().to_string()),
        ])
        .build();
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(SpanExporter::builder().build()?)
        .with_resource(resource.clone())
        .build();
    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(MetricExporter::builder().build()?)
        .with_resource(resource.clone())
        .build();
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(LogExporter::builder().build()?)
        .with_resource(resource)
        .build();
    global::set_meter_provider(meter_provider.clone());
    opentelemetry_instrumentation_process::init()?;

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(filter::filter_fn(|x| {
            !matches!(x.target(), "toasty::query" | "tokio_postgres::query")
        }))
        .with(fmt::layer())
        .with(
            tracing_opentelemetry::layer()
                .with_tracked_inactivity(false)
                .with_target(false)
                .with_tracer(tracer_provider.tracer("selfless")),
        )
        .with(
            OpenTelemetryTracingBridge::new(&logger_provider).with_filter(filter::filter_fn(|x| {
                !["opentelemetry", "tonic", "h2", "hyper", "tower"]
                    .iter()
                    .any(|prefix| x.target().starts_with(prefix))
            })),
        )
        .init();

    Ok(Telemetry {
        tracer_provider,
        meter_provider,
        logger_provider,
    })
}

impl Telemetry {
    pub(crate) fn shutdown(self) {
        if let Err(error) = self.tracer_provider.shutdown() {
            tracing::warn!(error = &error as &dyn Error, "failed to flush spans");
        }
        if let Err(error) = self.meter_provider.shutdown() {
            tracing::warn!(error = &error as &dyn Error, "failed to flush metrics");
        }
        if let Err(error) = self.logger_provider.shutdown() {
            tracing::warn!(error = &error as &dyn Error, "failed to flush logs");
        }
    }
}
