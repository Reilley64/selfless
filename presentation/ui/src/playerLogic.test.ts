import { createPlayerLogic, type ProgressReport } from "./playerLogic";

beforeEach(() => jest.useFakeTimers());
afterEach(() => jest.useRealTimers());

function player() {
  const reports: ProgressReport[] = [];
  return { reports, logic: createPlayerLogic((report) => reports.push(report)) };
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
