export type PlayerEvent =
  | { type: "playing"; position: number }
  | { type: "paused"; position: number }
  | { type: "position"; position: number }
  | { type: "stopped"; position: number };

export type ProgressReport = { position: number; paused: boolean; stopped: boolean };

export const reportInterval = 10_000;

export function createPlayerLogic(sendReport: (report: ProgressReport) => void) {
  let position = 0;
  let paused = false;
  let stopped = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  const report = () => sendReport({ position, paused, stopped });
  const restartTimer = () => {
    clearInterval(timer);
    timer = setInterval(report, reportInterval);
  };
  return {
    handle(event: PlayerEvent) {
      if (stopped) {
        return;
      }
      position = event.position;
      switch (event.type) {
        case "playing":
          paused = false;
          if (timer === undefined) {
            restartTimer();
          }
          break;
        case "paused":
          paused = true;
          report();
          restartTimer();
          break;
        case "stopped":
          stopped = true;
          clearInterval(timer);
          report();
          break;
      }
    },
  };
}
