import { type ProxyTracerProvider, logs, metrics, trace } from "@opentelemetry/api";
import type { LoggerProvider } from "@opentelemetry/sdk-logs";
import type { MeterProvider } from "@opentelemetry/sdk-metrics";
import type { TracerProvider } from "@opentelemetry/sdk-trace";
import { type SessionStorage, startWebSession } from "../session";
import { startTelemetry } from "./startTelemetry";

const token = "q2Xz8kQ0mJ3Hc1yF5bR7tVw9nL4pA6sD0eG2iK8uO1M";
const user = { id: "01890a5d-ac96-774b-bcce-b302099a8057", name: "Noor", colour: "amber", symbol: "ring" };

function fakeStorage(): SessionStorage {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
    removeItem: (key) => void items.delete(key),
  };
}

function mockServer() {
  return jest.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    if (String(input).endsWith("/api/sessions/v1")) {
      return Response.json({ token, user }, { status: 201 });
    }
    return new Response(null, { status: init?.method === "DELETE" ? 204 : 200 });
  });
}

function providers() {
  return [
    (trace.getTracerProvider() as ProxyTracerProvider).getDelegate() as TracerProvider,
    metrics.getMeterProvider() as MeterProvider,
    logs.getLoggerProvider() as LoggerProvider,
  ];
}

async function flush() {
  await Promise.all(providers().map((provider) => provider.forceFlush()));
}

function record(name: string) {
  trace.getTracer("test").startSpan(name).end();
  logs.getLogger("test").emit({ body: name });
  metrics.getMeter("test").createCounter("test.recorded").add(1);
}

function telemetryCalls(fetch: ReturnType<typeof mockServer>) {
  return fetch.mock.calls.filter(([input]) => String(input).includes("/api/telemetry/"));
}

beforeAll(() => {
  Object.defineProperty(globalThis, "location", { value: new URL("http://localhost:8081/"), configurable: true });
});

afterEach(async () => {
  await Promise.all(providers().map((provider) => provider.shutdown()));
  trace.disable();
  metrics.disable();
  logs.disable();
  jest.restoreAllMocks();
});

test("nothing is sent before anyone signs in", async () => {
  const fetch = mockServer();
  startTelemetry("selfless-web", startWebSession(fakeStorage()));

  record("before sign-in");
  await flush();

  expect(telemetryCalls(fetch)).toEqual([]);
});

function sentText(call: [unknown, RequestInit?]) {
  return Buffer.from(call[1]?.body as Uint8Array).toString();
}

test("telemetry recorded before sign-in is not sent after sign-in", async () => {
  const fetch = mockServer();
  const session = startWebSession(fakeStorage());
  startTelemetry("selfless-web", session);

  record("before sign-in");
  await session.signIn("Noor", "correct horse", "Firefox on macOS");
  record("after sign-in");
  await flush();

  const sent = telemetryCalls(fetch).map(sentText).join();
  expect(sent).toContain("after sign-in");
  expect(sent).not.toContain("before sign-in");
  expect(sent).not.toContain("/api/sessions/v1");
});

test("telemetry recorded after sign-in goes out with the session token", async () => {
  const fetch = mockServer();
  const session = startWebSession(fakeStorage());
  startTelemetry("selfless-web", session);
  await session.signIn("Noor", "correct horse", "Firefox on macOS");

  record("after sign-in");
  await flush();

  expect(telemetryCalls(fetch).map(([input, init]) => [input, new Headers(init?.headers).get("Authorization")])).toEqual(
    expect.arrayContaining(
      ["traces", "metrics", "logs"].map((signal) => [`http://localhost:8081/api/telemetry/${signal}/v1`, `Bearer ${token}`]),
    ),
  );
});
