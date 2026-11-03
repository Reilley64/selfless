import { SpanStatusCode, context, trace } from "@opentelemetry/api";
import {
  InMemorySpanExporter,
  type ReadableSpan,
  SimpleSpanProcessor,
  StackContextManager,
  TracerProvider,
} from "@opentelemetry/sdk-trace";
import { createPlayerLogic, type ProgressReport, startPlaybackSpans } from "./playerLogic";

const userId = "01890a5d-ac96-774b-bcce-b302099a8057";
const mediaFileId = "01890a5d-ac96-774b-bcce-b302099a8060";
const exporter = new InMemorySpanExporter();

beforeAll(() => {
  context.setGlobalContextManager(new StackContextManager().enable());
  trace.setGlobalTracerProvider(new TracerProvider({ spanProcessors: [new SimpleSpanProcessor({ exporter })] }));
});
afterAll(() => {
  trace.disable();
  context.disable();
});
beforeEach(() => jest.useFakeTimers());
afterEach(() => {
  jest.useRealTimers();
  exporter.reset();
});

function player() {
  const reports: ProgressReport[] = [];
  const spans = startPlaybackSpans(userId, mediaFileId);
  return { reports, spans, logic: createPlayerLogic((report) => reports.push(report), spans) };
}

function playbackSpans() {
  return exporter.getFinishedSpans().filter((span) => span.name === "selfless.playback");
}

function events(spans: ReadableSpan[]) {
  return spans.map((span) => span.events.map((event) => event.name));
}

test("while playing, a report goes every 10 seconds with the latest position", () => {
  const { reports, logic } = player();

  logic.handle({ type: "playing", position: 0 });
  jest.advanceTimersByTime(9_999);
  const beforeTenSeconds = reports.length;
  logic.handle({ type: "position", position: 9.9 });
  jest.advanceTimersByTime(1);
  logic.handle({ type: "position", position: 19.9 });
  jest.advanceTimersByTime(10_000);

  expect(beforeTenSeconds).toBe(0);
  expect(reports).toEqual([
    { position: 9.9, paused: false, stopped: false },
    { position: 19.9, paused: false, stopped: false },
  ]);
});

test("a pause reports at once, then every 10 seconds while paused", () => {
  const { reports, logic } = player();
  logic.handle({ type: "playing", position: 0 });
  jest.advanceTimersByTime(4_000);

  logic.handle({ type: "paused", position: 4 });
  const atPause = [...reports];
  jest.advanceTimersByTime(10_000);

  expect(atPause).toEqual([{ position: 4, paused: true, stopped: false }]);
  expect(reports).toEqual([
    { position: 4, paused: true, stopped: false },
    { position: 4, paused: true, stopped: false },
  ]);
});

test("a stop reports at once and ends the reports", () => {
  const { reports, logic } = player();
  logic.handle({ type: "playing", position: 0 });
  jest.advanceTimersByTime(3_000);

  logic.handle({ type: "stopped", position: 3 });
  logic.handle({ type: "paused", position: 3 });
  jest.advanceTimersByTime(30_000);

  expect(reports).toEqual([{ position: 3, paused: false, stopped: true }]);
});

test("nothing is reported before playback starts", () => {
  const { reports, logic } = player();

  logic.handle({ type: "position", position: 0 });
  jest.advanceTimersByTime(30_000);

  expect(reports).toEqual([]);
});

test("each player event goes on its own playback span in the trace of the play request", () => {
  const { logic } = player();

  logic.handle({ type: "playing", position: 0 });
  logic.handle({ type: "paused", position: 5 });
  logic.handle({ type: "seeked", position: 30 });
  logic.handle({ type: "playing", position: 30 });
  logic.handle({ type: "stopped", position: 40 });

  const spans = playbackSpans();
  const [first] = spans;
  expect(events(spans)).toEqual([
    ["selfless.playback.started"],
    ["selfless.playback.paused"],
    ["selfless.playback.seeked"],
    ["selfless.playback.stopped"],
  ]);
  expect(spans.map((span) => span.spanContext().traceId)).toEqual(spans.map(() => first?.spanContext().traceId));
  expect(spans.slice(1).map((span) => span.parentSpanContext?.spanId)).toEqual(
    spans.slice(1).map(() => first?.spanContext().spanId),
  );
  expect(spans.map((span) => span.attributes)).toEqual(
    spans.map(() => ({ "user.id": userId, "selfless.media_file.id": mediaFileId })),
  );
});

test("the play request's span is a child of the first playback span", () => {
  const { spans, logic } = player();

  spans.during(() => trace.getTracer("test").startSpan("POST").end());
  logic.handle({ type: "playing", position: 0 });

  const [request] = exporter.getFinishedSpans().filter((span) => span.name === "POST");
  const [first] = playbackSpans();
  expect(first).toBeDefined();
  expect(request?.spanContext().traceId).toBe(first?.spanContext().traceId);
  expect(request?.parentSpanContext?.spanId).toBe(first?.spanContext().spanId);
});

test("the seek to the start position before playback starts is not recorded", () => {
  const { logic } = player();

  logic.handle({ type: "seeked", position: 120 });
  logic.handle({ type: "playing", position: 120 });

  expect(events(playbackSpans())).toEqual([["selfless.playback.started"]]);
});

test("a failure records failed on an error span with its error type", () => {
  const { logic } = player();
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "failed", position: 12, errorType: "MEDIA_ERR_DECODE" });

  const failed = (playbackSpans())[1];
  expect(failed?.events.map((event) => event.name)).toEqual(["selfless.playback.failed"]);
  expect(failed?.attributes["error.type"]).toBe("MEDIA_ERR_DECODE");
  expect(failed?.status.code).toBe(SpanStatusCode.ERROR);
});

test("a play request that fails ends the first playback span with failed", () => {
  const { spans } = player();

  spans.failed("422");

  const finished = playbackSpans();
  const [first] = finished;
  expect(events(finished)).toEqual([["selfless.playback.failed"]]);
  expect(first?.attributes["error.type"]).toBe("422");
  expect(first?.status.code).toBe(SpanStatusCode.ERROR);
});
