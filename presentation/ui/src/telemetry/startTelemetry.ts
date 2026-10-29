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

function whileSignedIn<Items>(session: TelemetrySession, exporter: Exporter<Items>): Required<Exporter<Items>> {
  return {
    export(items, resultCallback) {
      if (session.token()) {
        exporter.export(items, resultCallback);
      } else {
        resultCallback({ code: ExportResultCode.SUCCESS });
      }
    },
    forceFlush: async () => exporter.forceFlush?.(),
    shutdown: () => exporter.shutdown(),
  };
}

export function startTelemetry(serviceName: string, session: TelemetrySession) {
  const serverUrl = session.serverUrl();
  const headers = async () => authorizationHeaders(session.token());
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
          exporter: whileSignedIn(
            session,
            new OTLPTraceExporter({ url: `${serverUrl}/api/telemetry/traces/v1`, headers }),
          ),
        }),
      ],
    }),
  );
  metrics.setGlobalMeterProvider(
    new MeterProvider({
      resource,
      readers: [
        new PeriodicExportingMetricReader({
          exporter: whileSignedIn(
            session,
            new OTLPMetricExporter({ url: `${serverUrl}/api/telemetry/metrics/v1`, headers }),
          ),
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
          exporter: whileSignedIn(session, new OTLPLogExporter({ url: `${serverUrl}/api/telemetry/logs/v1`, headers })),
        }),
      ],
    }),
  );
}
