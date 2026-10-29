import { logs, metrics, propagation, trace } from "@opentelemetry/api";
import { type ExportResult, ExportResultCode, W3CTraceContextPropagator } from "@opentelemetry/core";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-proto";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-proto";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-proto";
import { defaultResource, resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider, type ReadWriteLogRecord } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BatchSpanProcessor, type ReadableSpan, TracerProvider } from "@opentelemetry/sdk-trace";
import { ATTR_SERVICE_NAME } from "@opentelemetry/semantic-conventions";
import { authorizationHeaders } from "@selfless/api-client";

interface TelemetrySession {
  serverUrl(): string;
  token(): string | undefined;
}

interface Exporter<Items> {
  export(items: Items, resultCallback: (result: ExportResult) => void): void;
  forceFlush?(): Promise<void>;
  shutdown(): Promise<void>;
}

function sessionExporter<Items>(
  session: TelemetrySession,
  signal: string,
  OtlpExporter: new (config: { url: string; headers: () => Promise<Record<string, string>> }) => Exporter<Items>,
): Required<Exporter<Items>> {
  let url: string | undefined;
  let exporter: Exporter<Items> | undefined;
  return {
    export(items, resultCallback) {
      if (!session.token()) {
        resultCallback({ code: ExportResultCode.SUCCESS });
        return;
      }
      const current = `${session.serverUrl()}/api/telemetry/${signal}/v1`;
      if (!exporter || url !== current) {
        void exporter?.shutdown();
        url = current;
        exporter = new OtlpExporter({ url, headers: async () => authorizationHeaders(session.token()) });
      }
      exporter.export(items, resultCallback);
    },
    forceFlush: async () => exporter?.forceFlush?.(),
    shutdown: async () => exporter?.shutdown(),
  };
}

export function startTelemetry(serviceName: string, session: TelemetrySession) {
  const resource = defaultResource().merge(resourceFromAttributes({ [ATTR_SERVICE_NAME]: serviceName }));
  propagation.setGlobalPropagator(new W3CTraceContextPropagator());
  trace.setGlobalTracerProvider(
    new TracerProvider({
      resource,
      spanProcessors: [
        new (class extends BatchSpanProcessor {
          onEnd(span: ReadableSpan) {
            if (session.token()) {
              super.onEnd(span);
            }
          }
        })({
          exporter: sessionExporter(session, "traces", OTLPTraceExporter),
        }),
      ],
    }),
  );
  metrics.setGlobalMeterProvider(
    new MeterProvider({
      resource,
      readers: [
        new PeriodicExportingMetricReader({
          exporter: sessionExporter(session, "metrics", OTLPMetricExporter),
        }),
      ],
    }),
  );
  logs.setGlobalLoggerProvider(
    new LoggerProvider({
      resource,
      processors: [
        new (class extends BatchLogRecordProcessor {
          onEmit(logRecord: ReadWriteLogRecord) {
            if (session.token()) {
              super.onEmit(logRecord);
            }
          }
        })({
          exporter: sessionExporter(session, "logs", OTLPLogExporter),
        }),
      ],
    }),
  );
}
