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
  PlaybackNextEpisode,
  PlaybackSubtitleTrack,
  reportPlaybackProgressResponse,
} from "@selfless/api-client";
import {
  type NextEpisodeButtonState,
  type ProgressReport,
  type Resume,
  createPlayerLogic,
  matchTrack,
  playerTrackFor,
  startPlaybackSpans,
} from "./playerLogic";

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

function audioTrack(id: string, overrides: Partial<PlaybackAudioTrack> = {}): PlaybackAudioTrack {
  return { id, language: "eng", label: null, codec: "aac", channels: 2, default: false, inStream: true, ...overrides };
}

function subtitleTrack(id: string, overrides: Partial<PlaybackSubtitleTrack> = {}): PlaybackSubtitleTrack {
  return {
    id,
    language: "eng",
    label: null,
    codec: "subrip",
    default: false,
    forced: false,
    hearingImpaired: false,
    external: false,
    text: true,
    inStream: true,
    ...overrides,
  };
}

const english = audioTrack("english", { default: true });
const japanese = audioTrack("japanese", { language: "jpn" });
const tracks = { audioTrackId: "english", subtitleTrackId: null };
const nextEpisode: PlaybackNextEpisode = {
  id: "s01e04",
  mediaFileId: "01890a5d-ac96-774b-bcce-b302099a8061",
  seasonNumber: 1,
  episodeNumber: 4,
  title: "The You You Are",
  thumb: null,
};

function player(
  response = reported,
  {
    method = "directPlay",
    forceHls = false,
    audioTrackId = "english",
    audioSwitchesInPlace = true,
    nextEpisode = null,
    autoplayNextEpisode = true,
  }: {
    method?: PlayMethod;
    forceHls?: boolean;
    audioTrackId?: string;
    audioSwitchesInPlace?: boolean;
    nextEpisode?: PlaybackNextEpisode | null;
    autoplayNextEpisode?: boolean;
  } = {},
) {
  const reports: ProgressReport[] = [];
  const restarts: Resume[] = [];
  const errors: number[] = [];
  const switches: (string | null)[] = [];
  const buttons: (NextEpisodeButtonState | null)[] = [];
  const endings: string[] = [];
  const spans = startPlaybackSpans(userId, mediaFileId);
  const logic = createPlayerLogic({
    playback: {
      method,
      duration: 1_500,
      audioTracks: [english, japanese],
      audioTrackId,
      subtitleTrackId: null,
      nextEpisode,
      autoplayNextEpisode,
    },
    audioSwitchesInPlace,
    forceHls,
    sendReport: async (report) => {
      reports.push(report);
      return response;
    },
    restart: (resume) => restarts.push(resume),
    showError: () => errors.push(errors.length),
    switchAudio: (track) => switches.push(track.id),
    switchSubtitle: (track) => switches.push(track?.id ?? null),
    showNextEpisode: (button) => buttons.push(button),
    playNextEpisode: (episode) => endings.push(episode.id),
    returnToTitle: () => endings.push("title page"),
    spans,
  });
  return { reports, restarts, errors, switches, buttons, endings, spans, logic };
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
    { position: 9.9, paused: false, stopped: false, ...tracks },
    { position: 19.9, paused: false, stopped: false, ...tracks },
  ]);
});

test("a pause reports at once, then every 10 seconds while paused", () => {
  const { reports, logic } = player();
  logic.handle({ type: "playing", position: 0 });
  jest.advanceTimersByTime(4_000);

  logic.handle({ type: "paused", position: 4 });
  const atPause = [...reports];
  jest.advanceTimersByTime(10_000);

  expect(atPause).toEqual([{ position: 4, paused: true, stopped: false, ...tracks }]);
  expect(reports).toEqual([
    { position: 4, paused: true, stopped: false, ...tracks },
    { position: 4, paused: true, stopped: false, ...tracks },
  ]);
});

test("a stop reports at once and ends the reports", () => {
  const { reports, logic } = player();
  logic.handle({ type: "playing", position: 0 });
  jest.advanceTimersByTime(3_000);

  logic.handle({ type: "stopped", position: 3 });
  logic.handle({ type: "paused", position: 3 });
  jest.advanceTimersByTime(30_000);

  expect(reports).toEqual([{ position: 3, paused: false, stopped: true, ...tracks }]);
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

  expect(playing.restarts).toEqual([{ position: 9.9, paused: false, forceHls: false, tracks }]);
  expect(paused.restarts).toEqual([{ position: 7, paused: true, forceHls: false, tracks }]);
});

test("a playback that ended after a switch to HLS starts again through HLS", async () => {
  const { restarts, logic } = player(ended, { method: "remux", forceHls: true });
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "position", position: 9.9 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(restarts).toEqual([{ position: 9.9, paused: false, forceHls: true, tracks }]);
});

test("a direct play error starts the playback again through HLS at the same position", async () => {
  const { reports, restarts, errors, logic } = player();
  logic.handle({ type: "playing", position: 0 });

  logic.handle({ type: "failed", position: 42, errorType: "MEDIA_ERR_SRC_NOT_SUPPORTED" });
  await jest.advanceTimersByTimeAsync(30_000);

  const switched = playbackSpans()[1];
  expect(restarts).toEqual([{ position: 42, paused: false, forceHls: true, tracks }]);
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

test("an audio track in the stream switches in place, and the next report carries it", async () => {
  const { reports, restarts, switches, logic } = player(reported, { method: "remux" });
  logic.handle({ type: "playing", position: 0 });

  logic.selectAudio(japanese);
  logic.handle({ type: "position", position: 9.9 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(switches).toEqual(["japanese"]);
  expect(restarts).toEqual([]);
  expect(reports).toEqual([
    { position: 9.9, paused: false, stopped: false, audioTrackId: "japanese", subtitleTrackId: null },
  ]);
});

test("a subtitle in the stream turns on in place, and turning subtitles off switches in place too", () => {
  const { restarts, switches, logic } = player(reported, { method: "remux" });
  logic.handle({ type: "playing", position: 0 });

  logic.selectSubtitle(subtitleTrack("english"));
  logic.selectSubtitle(null);

  expect(switches).toEqual(["english", null]);
  expect(restarts).toEqual([]);
});

test("a subtitle outside the stream starts a new playback at the current position and records a switch", () => {
  const { restarts, switches, logic } = player();
  logic.handle({ type: "playing", position: 0 });
  logic.handle({ type: "position", position: 61 });

  logic.selectSubtitle(subtitleTrack("external", { external: true, inStream: false }));
  logic.handle({ type: "paused", position: 61 });

  const switched = playbackSpans()[1];
  expect(switches).toEqual([]);
  expect(restarts).toEqual([
    { position: 61, paused: false, forceHls: false, tracks: { audioTrackId: "english", subtitleTrackId: "external" } },
  ]);
  expect(switched?.events.map((event) => event.name)).toEqual(["selfless.playback.switched"]);
  expect(switched?.attributes["error.type"]).toBeUndefined();
});

test("an audio track the player can't switch to in place starts a new playback through HLS", () => {
  const { restarts, switches, logic } = player(reported, { audioSwitchesInPlace: false });
  logic.handle({ type: "playing", position: 30 });

  logic.selectAudio(japanese);

  expect(switches).toEqual([]);
  expect(restarts).toEqual([
    { position: 30, paused: false, forceHls: true, tracks: { audioTrackId: "japanese", subtitleTrackId: null } },
  ]);
});

test("picking the track that already plays changes nothing", () => {
  const { restarts, switches, logic } = player();

  logic.selectAudio(english);
  logic.selectSubtitle(null);

  expect(switches).toEqual([]);
  expect(restarts).toEqual([]);
});

test("a direct play of a selected audio track that is not the file's default switches to it", () => {
  const inPlace = player(reported, { audioTrackId: "japanese" });
  const notInPlace = player(reported, { audioTrackId: "japanese", audioSwitchesInPlace: false });
  const theDefault = player(reported, { audioSwitchesInPlace: false });

  for (const { logic } of [inPlace, notInPlace, theDefault]) {
    logic.applySelectedAudio();
  }

  expect(inPlace.switches).toEqual(["japanese"]);
  expect(inPlace.restarts).toEqual([]);
  expect(notInPlace.restarts).toEqual([
    { position: 0, paused: false, forceHls: true, tracks: { audioTrackId: "japanese", subtitleTrackId: null } },
  ]);
  expect(theDefault.restarts).toEqual([]);
});

test("a player track matches the API track in its language, whether the language has two or three letters", () => {
  const french = { language: "fr", label: "French" };
  const brazilian = { language: "pt_BR", label: "Portuguese (BR)" };
  const players = [french, brazilian];
  const tracks = [audioTrack("eng", { label: "Stereo" }), audioTrack("fre", { language: "fre", label: "Stereo" }), audioTrack("pt", { language: "pt-BR" })];

  expect([matchTrack(french, players, tracks)?.id, matchTrack(brazilian, players, tracks)?.id]).toEqual(["fre", "pt"]);
});

test("player tracks in one language match the API tracks in that language in order, told apart by label", () => {
  const players = [
    { language: "en", label: "English (Stereo)" },
    { language: "ja", label: "Japanese" },
    { language: "en", label: "English (Commentary)" },
  ];
  const tracks = [audioTrack("stereo", { label: "Stereo" }), audioTrack("japanese", { language: "jpn" }), audioTrack("commentary", { label: "Commentary" })];

  expect(matchTrack({ language: "en", label: "English (Commentary)" }, players, tracks)?.id).toBe("commentary");
});

test("a player track matches no API track the stream doesn't carry", () => {
  const english = { language: "en", label: "English" };
  const tracks = [subtitleTrack("pgs", { codec: "hdmv_pgs_subtitle", text: false, inStream: false }), subtitleTrack("srt", { language: "en", codec: "srt", external: true })];

  expect([matchTrack(english, [english], tracks)?.id, matchTrack({ language: "de", label: "German" }, [english], tracks)]).toEqual(["srt", undefined]);
});

test("an API track finds the player track in its place among the player tracks of its language", () => {
  const players = [
    { language: "en", label: "English (Stereo)" },
    { language: "ja", label: "Japanese" },
    { language: "en", label: "English (Commentary)" },
  ];
  const tracks = [
    audioTrack("stereo", { label: "Stereo" }),
    audioTrack("japanese", { language: "jpn" }),
    audioTrack("commentary", { label: "Commentary" }),
    audioTrack("german", { language: "ger" }),
  ];

  expect(tracks.map((track) => playerTrackFor(track, players, tracks)?.label)).toEqual([
    "English (Stereo)",
    "Japanese",
    "English (Commentary)",
    undefined,
  ]);
});

test("the Next episode button shows 5 seconds before the end and counts down the seconds left", () => {
  const { buttons, logic } = player(reported, { nextEpisode });
  logic.handle({ type: "playing", position: 1_490 });

  for (const position of [1_494.9, 1_495, 1_495.5, 1_496.2, 1_497, 1_499.8]) {
    jest.advanceTimersByTime(500);
    logic.handle({ type: "position", position });
  }

  expect(buttons).toEqual([{ countdown: 5 }, { countdown: 4 }, { countdown: 3 }, { countdown: 1 }]);
});

test("with autoplay on, the next episode plays when the file ends, after a stop report", async () => {
  const { reports, buttons, endings, logic } = player(reported, { nextEpisode });
  logic.handle({ type: "playing", position: 1_498 });

  logic.handle({ type: "ended", position: 1_500 });
  logic.handle({ type: "stopped", position: 1_500 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(endings).toEqual(["s01e04"]);
  expect(reports).toEqual([{ position: 1_500, paused: false, stopped: true, ...tracks }]);
  expect(buttons).toEqual([{ countdown: 2 }, null]);
});

test("with autoplay off, the button shows without a countdown and the end of the file returns to the show page", () => {
  const { buttons, endings, logic } = player(reported, { nextEpisode, autoplayNextEpisode: false });
  logic.handle({ type: "playing", position: 1_490 });

  for (const position of [1_495, 1_497, 1_499]) {
    logic.handle({ type: "position", position });
  }
  logic.handle({ type: "ended", position: 1_500 });

  expect(buttons).toEqual([{ countdown: null }, null]);
  expect(endings).toEqual(["title page"]);
});

test("pressing the Next episode button plays the next episode at once, after a stop report", async () => {
  const { reports, endings, logic } = player(reported, { nextEpisode });
  logic.handle({ type: "playing", position: 1_496 });

  logic.playNextEpisode();
  logic.handle({ type: "ended", position: 1_500 });
  await jest.advanceTimersByTimeAsync(10_000);

  expect(endings).toEqual(["s01e04"]);
  expect(reports).toEqual([{ position: 1_496, paused: false, stopped: true, ...tracks }]);
});

test("after a movie or a show's last episode, the end of the file returns to its page and no button shows", () => {
  const { buttons, endings, logic } = player();
  logic.handle({ type: "playing", position: 1_490 });

  logic.handle({ type: "position", position: 1_498 });
  logic.handle({ type: "ended", position: 1_500 });

  expect(buttons).toEqual([]);
  expect(endings).toEqual(["title page"]);
});

test("seeking back before the last 5 seconds hides the button, and leaving the player goes nowhere", () => {
  const { buttons, endings, logic } = player(reported, { nextEpisode });
  logic.handle({ type: "playing", position: 1_496 });

  logic.handle({ type: "seeked", position: 1_200 });
  logic.handle({ type: "stopped", position: 1_200 });

  expect(buttons).toEqual([{ countdown: 4 }, null]);
  expect(endings).toEqual([]);
});
