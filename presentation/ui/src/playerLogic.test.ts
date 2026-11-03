import { SpanStatusCode, context, trace } from "@opentelemetry/api";
import {
  InMemorySpanExporter,
  type ReadableSpan,
  SimpleSpanProcessor,
  StackContextManager,
  TracerProvider,
} from "@opentelemetry/sdk-trace";
import type {
  PlayMethod,
  PlaybackAudioTrack,
  PlaybackSubtitleTrack,
  reportPlaybackProgressResponse,
} from "@selfless/api-client";
import { type ProgressReport, type Resume, createPlayerLogic, matchTrack, startPlaybackSpans } from "./playerLogic";

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

const headers = new Headers();
const reported: reportPlaybackProgressResponse = { status: 204, data: undefined, headers };
const ended: reportPlaybackProgressResponse = {
  status: 404,
  data: { type: "urn:selfless:problem:playback-ended", title: "The playback has ended.", status: 404 },
  headers,
};
const notFound: reportPlaybackProgressResponse = {
  status: 404,
  data: { type: "urn:selfless:problem:not-found", title: "Nothing exists at this URL.", status: 404 },
  headers,
};

function player(
  response = reported,
  { method = "directPlay", forceHls = false }: { method?: PlayMethod; forceHls?: boolean } = {},
) {
  const reports: ProgressReport[] = [];
  const restarts: Resume[] = [];
  const errors: number[] = [];
  const spans = startPlaybackSpans(userId, mediaFileId);
  const logic = createPlayerLogic({
    method,
    forceHls,
    sendReport: async (report) => {
      reports.push(report);
      return response;
    },
    restart: (resume) => restarts.push(resume),
    showError: () => errors.push(errors.length),
    spans,
  });
  return { reports, restarts, errors, spans, logic };
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

test("a report on an ended playback starts a new playback at the position, paused or not", async () => {
  const playing = player(ended);
  const paused = player(ended);
  playing.logic.handle({ type: "playing", position: 0 });
  paused.logic.handle({ type: "playing", position: 0 });

  playing.logic.handle({ type: "position", position: 9.9 });
  paused.logic.handle({ type: "paused", position: 7 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(playing.restarts).toEqual([{ position: 9.9, paused: false, forceHls: false }]);
  expect(paused.restarts).toEqual([{ position: 7, paused: true, forceHls: false }]);
});

test("a playback that ended after a switch to HLS starts again through HLS", async () => {
  const { restarts, logic } = player(ended, { method: "remux", forceHls: true });
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "position", position: 9.9 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(restarts).toEqual([{ position: 9.9, paused: false, forceHls: true }]);
});

test("a direct play error starts the playback again through HLS at the same position", async () => {
  const { reports, restarts, errors, logic } = player();
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "failed", position: 42, errorType: "MEDIA_ERR_SRC_NOT_SUPPORTED" });
  await jest.advanceTimersByTimeAsync(30_000);

  const switched = playbackSpans()[1];
  expect(restarts).toEqual([{ position: 42, paused: false, forceHls: true }]);
  expect(errors).toEqual([]);
  expect(reports).toEqual([]);
  expect(switched?.events.map((event) => event.name)).toEqual(["selfless.playback.switched"]);
  expect(switched?.attributes["error.type"]).toBe("MEDIA_ERR_SRC_NOT_SUPPORTED");
  expect(switched?.status.code).toBe(SpanStatusCode.UNSET);
});

test("an HLS playback error shows the error and starts no new playback", () => {
  const { restarts, errors, logic } = player(reported, { method: "remux", forceHls: true });
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "failed", position: 42, errorType: "MEDIA_ERR_DECODE" });

  expect(restarts).toEqual([]);
  expect(errors).toHaveLength(1);
});

test("after the playback ended, the old playback sends no more reports", async () => {
  const { reports, restarts, logic } = player(ended);
  logic.handle({ type: "playing", position: 0 });
  await jest.advanceTimersByTimeAsync(10_000);

  logic.handle({ type: "paused", position: 12 });
  await jest.advanceTimersByTimeAsync(30_000);

  expect(reports).toHaveLength(1);
  expect(restarts).toHaveLength(1);
});

test("a stop report on an ended playback starts no new playback", async () => {
  const { restarts, logic } = player(ended);
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "stopped", position: 3 });
  await jest.advanceTimersByTimeAsync(0);

  expect(restarts).toEqual([]);
});

test("a playback that is not found starts no new playback", async () => {
  const { reports, restarts, logic } = player(notFound);
  logic.handle({ type: "playing", position: 0 });

  await jest.advanceTimersByTimeAsync(20_000);

  expect(reports).toHaveLength(2);
  expect(restarts).toEqual([]);
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
  const { logic } = player(reported, { method: "remux" });
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "failed", position: 12, errorType: "MEDIA_ERR_DECODE" });

  const failed = playbackSpans()[1];
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

function audioTrack(id: string, language: string | null, label: string | null): PlaybackAudioTrack {
  return { id, language, label, codec: "aac", channels: 2, default: false, inStream: true };
}

function subtitleTrack(id: string, language: string, codec: string, inStream: boolean): PlaybackSubtitleTrack {
  return {
    id,
    language,
    label: null,
    codec,
    default: false,
    forced: false,
    hearingImpaired: false,
    external: codec === "srt",
    text: inStream,
    inStream,
  };
}

test("a player track matches the API track in its language, whether the language has two or three letters", () => {
  const french = { language: "fr", label: "French" };
  const brazilian = { language: "pt_BR", label: "Portuguese (BR)" };
  const players = [french, brazilian];
  const tracks = [audioTrack("eng", "eng", "Stereo"), audioTrack("fre", "fre", "Stereo"), audioTrack("pt", "pt-BR", null)];

  expect([matchTrack(french, players, tracks)?.id, matchTrack(brazilian, players, tracks)?.id]).toEqual(["fre", "pt"]);
});

test("player tracks in one language match the API tracks in that language in order, told apart by label", () => {
  const players = [
    { language: "en", label: "English (Stereo)" },
    { language: "ja", label: "Japanese" },
    { language: "en", label: "English (Commentary)" },
  ];
  const tracks = [audioTrack("stereo", "eng", "Stereo"), audioTrack("japanese", "jpn", null), audioTrack("commentary", "eng", "Commentary")];

  expect(matchTrack({ language: "en", label: "English (Commentary)" }, players, tracks)?.id).toBe("commentary");
});

test("a player track matches no API track the stream doesn't carry", () => {
  const english = { language: "en", label: "English" };
  const tracks = [subtitleTrack("pgs", "eng", "hdmv_pgs_subtitle", false), subtitleTrack("srt", "en", "srt", true)];

  expect([matchTrack(english, [english], tracks)?.id, matchTrack({ language: "de", label: "German" }, [english], tracks)]).toEqual(["srt", undefined]);
});
