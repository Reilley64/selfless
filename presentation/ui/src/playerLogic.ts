import type { reportPlaybackProgress } from "@selfless/api-client";

export type PlayerEvent =
  | { type: "playing"; position: number }
  | { type: "paused"; position: number }
  | { type: "position"; position: number }
  | { type: "stopped"; position: number };

export type ProgressReport = { position: number; paused: boolean; stopped: boolean };

export type Resume = { position: number; paused: boolean };

export const reportInterval = 10_000;

export function createPlayerLogic({
  sendReport,
  restart,
}: {
  sendReport: (report: ProgressReport) => ReturnType<typeof reportPlaybackProgress>;
  restart: (resume: Resume) => void;
}) {
  let position = 0;
  let paused = false;
  let stopped = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  const end = () => {
    stopped = true;
    clearInterval(timer);
  };
  const report = () => {
    const sent = { position, paused, stopped };
    sendReport(sent).then(
      (response) => {
        if (
          !sent.stopped &&
          !stopped &&
          response.status === 404 &&
          response.data.type === "urn:selfless:problem:playback-ended"
        ) {
          end();
          restart({ position, paused });
        }
      },
      () => undefined,
    );
  };
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
          end();
          report();
          break;
      }
    },
  };
}
