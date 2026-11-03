import type { reportPlaybackProgressResponse } from "@selfless/api-client";
import { type Resume, createPlayerLogic, type ProgressReport } from "./playerLogic";

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

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

function player(response = reported) {
  const reports: ProgressReport[] = [];
  const restarts: Resume[] = [];
  const logic = createPlayerLogic({
    sendReport: async (report) => {
      reports.push(report);
      return response;
    },
    restart: (resume) => restarts.push(resume),
  });
  return { reports, restarts, logic };
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

  expect(playing.restarts).toEqual([{ position: 9.9, paused: false }]);
  expect(paused.restarts).toEqual([{ position: 7, paused: true }]);
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
