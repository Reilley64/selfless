import { type ProxyTracerProvider, logs, metrics, trace } from "@opentelemetry/api";
import type { LoggerProvider } from "@opentelemetry/sdk-logs";
import type { MeterProvider } from "@opentelemetry/sdk-metrics";
import type { TracerProvider } from "@opentelemetry/sdk-trace";
import { getMe } from "@selfless/api-client";
import { type SessionStorage, startTvSession, startWebSession } from "../session";
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

const theo = {
  token: "Zt0bX9qL2wR5yV8nH3kM6pJ1sF4dG7aC0eU2iO5lN8Q",
  user: { id: "01890a5d-ac96-774b-bcce-b302099a8058", name: "Theo", colour: "blue", symbol: "triangle" },
};

function mockServer({ meStatus = 200 } = {}) {
  return jest.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    if (String(input).endsWith("/api/sessions/v1")) {
      const { name } = JSON.parse(String(init?.body));
      return Response.json(name === "Theo" ? theo : { token, user }, { status: 201 });
    }
    if (String(input).endsWith("/api/me/v1")) {
      return new Response(null, { status: meStatus });
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

function sentWith(fetch: ReturnType<typeof mockServer>, text: string) {
  return telemetryCalls(fetch)
    .filter((call) => sentText(call).includes(text))
    .map(([input, init]) => [new URL(String(input)).origin, new Headers(init?.headers).get("Authorization")]);
}

function calls(fetch: ReturnType<typeof mockServer>) {
  return fetch.mock.calls.map((call) =>
    String(call[0]).includes("/api/telemetry/") ? sentText(call) : `${call[1]?.method ?? "GET"} ${call[0]}`,
  );
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

  expect(
    telemetryCalls(fetch).map(([input, init]) => [input, new Headers(init?.headers).get("Authorization")]),
  ).toEqual(
    expect.arrayContaining(
      ["traces", "metrics", "logs"].map((signal) => [
        `http://localhost:8081/api/telemetry/${signal}/v1`,
        `Bearer ${token}`,
      ]),
    ),
  );
});

test("after the TV is pointed at another server, telemetry goes to the new server", async () => {
  const fetch = mockServer();
  const session = startTvSession(fakeStorage(), "appleTv");
  startTelemetry("selfless-tv", { serverUrl: session.serverUrl, token: session.telemetryToken });
  await session.connect("http://192.168.1.20:8080");
  await session.signIn("Noor", "correct horse", "Living Room");
  record("on the first server");
  await flush();
  await session.remove(user.id);
  await session.connect("http://192.168.1.30:8080");
  await session.signIn("Noor", "correct horse", "Living Room");

  record("on the second server");
  await flush();

  expect(sentWith(fetch, "on the first server")).toEqual([
    ["http://192.168.1.20:8080", `Bearer ${token}`],
    ["http://192.168.1.20:8080", `Bearer ${token}`],
  ]);
  expect(sentWith(fetch, "on the second server")).toEqual([
    ["http://192.168.1.30:8080", `Bearer ${token}`],
    ["http://192.168.1.30:8080", `Bearer ${token}`],
  ]);
});

test("telemetry recorded on the TV's user picker goes out with a signed-in user's token", async () => {
  const fetch = mockServer();
  const session = startTvSession(fakeStorage(), "appleTv");
  startTelemetry("selfless-tv", { serverUrl: session.serverUrl, token: session.telemetryToken });
  await session.connect("http://192.168.1.20:8080");
  await session.signIn("Noor", "correct horse", "Living Room");
  session.switchUser();

  record("on the picker");
  await flush();

  expect(sentWith(fetch, "on the picker")).toEqual([
    ["http://192.168.1.20:8080", `Bearer ${token}`],
    ["http://192.168.1.20:8080", `Bearer ${token}`],
  ]);
});

test("nothing is sent while every user on the TV's picker is dimmed", async () => {
  const fetch = mockServer({ meStatus: 401 });
  const session = startTvSession(fakeStorage(), "appleTv");
  startTelemetry("selfless-tv", { serverUrl: session.serverUrl, token: session.telemetryToken });
  await session.connect("http://192.168.1.20:8080");
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.refresh();

  record("all dimmed");
  await flush();

  expect(sentWith(fetch, "all dimmed")).toEqual([]);
});

test("switching user sends pending telemetry with the old user's token", async () => {
  const fetch = mockServer();
  const session = startTvSession(fakeStorage(), "appleTv");
  startTelemetry("selfless-tv", { serverUrl: session.serverUrl, token: session.telemetryToken });
  await session.connect("http://192.168.1.20:8080");
  await session.signIn("Noor", "correct horse", "Living Room");
  await session.signIn("Theo", "battery staple", "Living Room");
  record("as Theo");

  session.switchUser();
  await flush();

  expect(sentWith(fetch, "as Theo")).toEqual([
    ["http://192.168.1.20:8080", `Bearer ${theo.token}`],
    ["http://192.168.1.20:8080", `Bearer ${theo.token}`],
  ]);
});

test("signing out sends pending telemetry with the token before the session ends", async () => {
  const fetch = mockServer();
  const session = startWebSession(fakeStorage());
  startTelemetry("selfless-web", session);
  await session.signIn("Noor", "correct horse", "Firefox on macOS");
  record("before sign-out");

  await session.signOut();

  const sentAt = calls(fetch).findIndex((call) => call.includes("before sign-out"));
  expect(sentAt).toBeGreaterThan(-1);
  expect(sentAt).toBeLessThan(calls(fetch).indexOf("DELETE /api/sessions/current/v1"));
  expect(sentWith(fetch, "before sign-out")).toEqual([
    ["http://localhost:8081", `Bearer ${token}`],
    ["http://localhost:8081", `Bearer ${token}`],
  ]);
});

test("a 401 sends pending telemetry with the token before clearing it", async () => {
  const fetch = mockServer({ meStatus: 401 });
  const session = startWebSession(fakeStorage());
  startTelemetry("selfless-web", session);
  await session.signIn("Noor", "correct horse", "Firefox on macOS");
  record("before the 401");

  await getMe();
  await flush();

  expect(session.token()).toBeUndefined();
  expect(sentWith(fetch, "before the 401")).toEqual([
    ["http://localhost:8081", `Bearer ${token}`],
    ["http://localhost:8081", `Bearer ${token}`],
  ]);
});

test("signing in again on the TV sends pending telemetry with the old token before the sign-in ends it", async () => {
  const fetch = mockServer();
  const session = startTvSession(fakeStorage(), "appleTv");
  startTelemetry("selfless-tv", { serverUrl: session.serverUrl, token: session.telemetryToken });
  await session.connect("http://192.168.1.20:8080");
  await session.signIn("Noor", "correct horse", "Living Room");
  session.switchUser();
  record("before signing in again");

  await session.signIn("Noor", "correct horse", "Living Room");

  const sentAt = calls(fetch).findIndex((call) => call.includes("before signing in again"));
  expect(sentAt).toBeGreaterThan(-1);
  expect(sentAt).toBeLessThan(calls(fetch).lastIndexOf("POST http://192.168.1.20:8080/api/sessions/v1"));
  expect(sentWith(fetch, "before signing in again")).toEqual([
    ["http://192.168.1.20:8080", `Bearer ${token}`],
    ["http://192.168.1.20:8080", `Bearer ${token}`],
  ]);
});
