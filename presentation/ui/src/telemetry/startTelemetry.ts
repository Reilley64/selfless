import { logs, metrics, propagation, trace } from "@opentelemetry/api";
import { type ExportResult, ExportResultCode, W3CTraceContextPropagator } from "@opentelemetry/core";
import { OTLPLogExporter } from "@opentelemetry/exporter-logs-otlp-proto";
import { OTLPMetricExporter } from "@opentelemetry/exporter-metrics-otlp-proto";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-proto";
import { defaultResource, resourceFromAttributes } from "@opentelemetry/resources";
import { BatchLogRecordProcessor, LoggerProvider } from "@opentelemetry/sdk-logs";
import { MeterProvider, PeriodicExportingMetricReader } from "@opentelemetry/sdk-metrics";
import { BatchSpanProcessor, TracerProvider } from "@opentelemetry/sdk-trace";
import { ATTR_SERVICE_NAME } from "@opentelemetry/semantic-conventions";
import { authorizationHeaders, type Session } from "@selfless/api-client";

type TelemetrySession = Pick<Session, "serverUrl" | "token">;

interface Exporter<Items> {
  export(items: Items, resultCallback: (result: ExportResult) => void): void;
  forceFlush?(): Promise<void>;
  shutdown(): Promise<void>;
}

let providers: { forceFlush(): Promise<void> }[] = [];

export async function flushTelemetry() {
  await Promise.allSettled(providers.map((provider) => provider.forceFlush()));
}

function sessionExporter<Items>(
  session: TelemetrySession,
  signal: "traces" | "metrics" | "logs",
  OtlpExporter: new (config: { url: string; headers: () => Promise<Record<string, string>> }) => Exporter<Items>,
  byToken: (items: Items) => [string, Items][],
): Required<Exporter<Items>> {
  const exporters = new Map<string, Exporter<Items>>();
  const exporterFor = (token: string) => {
    const url = `${session.serverUrl()}/api/telemetry/${signal}/v1`;
    const key = `${url} ${token}`;
    let exporter = exporters.get(key);
    if (!exporter) {
      exporters.forEach((previous) => void previous.shutdown());
      exporters.clear();
      exporter = new OtlpExporter({ url, headers: async () => authorizationHeaders(token) });
      exporters.set(key, exporter);
    }
    return exporter;
  };
  return {
    export(items, resultCallback) {
      const batches = byToken(items);
      let remaining = batches.length;
      let failed: ExportResult | undefined;
      if (remaining === 0) {
        resultCallback({ code: ExportResultCode.SUCCESS });
      }
      for (const [token, batch] of batches) {
        exporterFor(token).export(batch, (result) => {
          failed = result.code === ExportResultCode.SUCCESS ? failed : result;
          remaining -= 1;
          if (remaining === 0) {
            resultCallback(failed ?? result);
          }
        });
      }
    },
    forceFlush: async () => {
      await Promise.all([...exporters.values()].map((exporter) => exporter.forceFlush?.()));
    },
    shutdown: async () => {
      await Promise.all([...exporters.values()].map((exporter) => exporter.shutdown()));
    },
  };
}

export function startTelemetry(serviceName: string, session: TelemetrySession) {
  const tokens = new WeakMap<object, string>();
  const rememberToken = (record: object) => {
    const token = session.token();
    if (token) {
      tokens.set(record, token);
    }
  };
  const tokenRecorder = {
    onStart() {},
    onEnd: rememberToken,
    onEmit: rememberToken,
    forceFlush: async () => {},
    shutdown: async () => {},
  };
  const byRememberedToken = <Item extends object>(items: Item[]) => {
    const batches = new Map<string, Item[]>();
    for (const item of items) {
      const token = tokens.get(item);
      if (token) {
        batches.set(token, [...(batches.get(token) ?? []), item]);
      }
    }
    return [...batches];
  };
  const resource = defaultResource().merge(resourceFromAttributes({ [ATTR_SERVICE_NAME]: serviceName }));
  propagation.setGlobalPropagator(new W3CTraceContextPropagator());
  const tracerProvider = new TracerProvider({
    resource,
    spanProcessors: [
      tokenRecorder,
      new BatchSpanProcessor({ exporter: sessionExporter(session, "traces", OTLPTraceExporter, byRememberedToken) }),
    ],
  });
  const loggerProvider = new LoggerProvider({
    resource,
    processors: [
      tokenRecorder,
      new BatchLogRecordProcessor({ exporter: sessionExporter(session, "logs", OTLPLogExporter, byRememberedToken) }),
    ],
  });
  providers = [tracerProvider, loggerProvider];
  trace.setGlobalTracerProvider(tracerProvider);
  logs.setGlobalLoggerProvider(loggerProvider);
  metrics.setGlobalMeterProvider(
    new MeterProvider({
      resource,
      readers: [
        new PeriodicExportingMetricReader({
          exporter: sessionExporter(session, "metrics", OTLPMetricExporter, (items) => {
            const token = session.token();
            return token ? [[token, items]] : [];
          }),
        }),
      ],
    }),
  );
}
