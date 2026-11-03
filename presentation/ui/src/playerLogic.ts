import { type Attributes, ROOT_CONTEXT, type Span, SpanStatusCode, context, trace } from "@opentelemetry/api";
import { ATTR_ERROR_TYPE } from "@opentelemetry/semantic-conventions";
import type { reportPlaybackProgress } from "@selfless/api-client";
import { SELFLESS_MEDIA_FILE_ID } from "./telemetry/attributes";
import {
  SELFLESS_PLAYBACK_FAILED,
  SELFLESS_PLAYBACK_PAUSED,
  SELFLESS_PLAYBACK_SEEKED,
  SELFLESS_PLAYBACK_STARTED,
  SELFLESS_PLAYBACK_STOPPED,
} from "./telemetry/events";
import { SELFLESS_PLAYBACK } from "./telemetry/spans";

export type PlayerEvent =
  | { type: "playing"; position: number }
  | { type: "paused"; position: number }
  | { type: "seeked"; position: number }
  | { type: "position"; position: number }
  | { type: "failed"; position: number; errorType: string }
  | { type: "stopped"; position: number };

export type ProgressReport = { position: number; paused: boolean; stopped: boolean };

export type Resume = { position: number; paused: boolean };

export type PlaybackSpans = ReturnType<typeof startPlaybackSpans>;

export const reportInterval = 10_000;

export function startPlaybackSpans(userId: string, mediaFileId: string) {
  const tracer = trace.getTracer("@selfless/ui");
  const attributes: Attributes = { "user.id": userId, [SELFLESS_MEDIA_FILE_ID]: mediaFileId };
  const first = tracer.startSpan(SELFLESS_PLAYBACK, { attributes }, ROOT_CONTEXT);
  const parent = trace.setSpan(ROOT_CONTEXT, first);
  let unused: Span | undefined = first;
  const spanFor = (event: string) => {
    const span = unused ?? tracer.startSpan(SELFLESS_PLAYBACK, { attributes }, parent);
    unused = undefined;
    span.addEvent(event);
    return span;
  };
  const end = (event: string) => () => spanFor(event).end();
  return {
    during<T>(request: () => T) {
      return context.with(parent, request);
    },
    started: end(SELFLESS_PLAYBACK_STARTED),
    paused: end(SELFLESS_PLAYBACK_PAUSED),
    seeked: end(SELFLESS_PLAYBACK_SEEKED),
    stopped: end(SELFLESS_PLAYBACK_STOPPED),
    failed(errorType: string) {
      const span = spanFor(SELFLESS_PLAYBACK_FAILED);
      span.setAttribute(ATTR_ERROR_TYPE, errorType);
      span.setStatus({ code: SpanStatusCode.ERROR });
      span.end();
    },
  };
}

export function createPlayerLogic({
  sendReport,
  restart,
  spans,
}: {
  sendReport: (report: ProgressReport) => ReturnType<typeof reportPlaybackProgress>;
  restart: (resume: Resume) => void;
  spans: PlaybackSpans;
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
            spans.started();
            restartTimer();
          }
          break;
        case "paused":
          paused = true;
          spans.paused();
          report();
          restartTimer();
          break;
        case "seeked":
          if (timer !== undefined) {
            spans.seeked();
          }
          break;
        case "failed":
          spans.failed(event.errorType);
          break;
        case "stopped":
          end();
          spans.stopped();
          report();
          break;
      }
    },
  };
}
