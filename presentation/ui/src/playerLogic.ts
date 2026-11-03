import { type Attributes, ROOT_CONTEXT, type Span, SpanStatusCode, context, trace } from "@opentelemetry/api";
import { ATTR_ERROR_TYPE } from "@opentelemetry/semantic-conventions";
import type {
  Playback,
  PlaybackAudioTrack,
  PlaybackSubtitleTrack,
  reportPlaybackProgress,
} from "@selfless/api-client";
import { SELFLESS_MEDIA_FILE_ID } from "./telemetry/attributes";
import {
  SELFLESS_PLAYBACK_FAILED,
  SELFLESS_PLAYBACK_PAUSED,
  SELFLESS_PLAYBACK_SEEKED,
  SELFLESS_PLAYBACK_STARTED,
  SELFLESS_PLAYBACK_STOPPED,
  SELFLESS_PLAYBACK_SWITCHED,
} from "./telemetry/events";
import { SELFLESS_PLAYBACK } from "./telemetry/spans";

export type PlayerEvent =
  | { type: "playing"; position: number }
  | { type: "paused"; position: number }
  | { type: "seeked"; position: number }
  | { type: "position"; position: number }
  | { type: "failed"; position: number; errorType: string }
  | { type: "stopped"; position: number };

export type Tracks = { audioTrackId: string | null; subtitleTrackId: string | null };

export type ProgressReport = { position: number; paused: boolean; stopped: boolean } & Tracks;

export type Resume = { position: number; paused: boolean; forceHls: boolean; tracks?: Tracks };

export type PlaybackSpans = ReturnType<typeof startPlaybackSpans>;

export type PlayerTrack = { language: string; label: string };

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
    switched(errorType?: string) {
      const span = spanFor(SELFLESS_PLAYBACK_SWITCHED);
      if (errorType !== undefined) {
        span.setAttribute(ATTR_ERROR_TYPE, errorType);
      }
      span.end();
    },
  };
}

export function createPlayerLogic({
  playback: { method, audioTracks, audioTrackId, subtitleTrackId },
  audioSwitchesInPlace,
  forceHls,
  sendReport,
  restart,
  showError,
  switchAudio,
  switchSubtitle,
  spans,
}: {
  playback: Pick<Playback, "method" | "audioTracks" | "audioTrackId" | "subtitleTrackId">;
  audioSwitchesInPlace: boolean;
  forceHls: boolean;
  sendReport: (report: ProgressReport) => ReturnType<typeof reportPlaybackProgress>;
  restart: (resume: Resume) => void;
  showError: () => void;
  switchAudio: (track: PlaybackAudioTrack) => void;
  switchSubtitle: (track: PlaybackSubtitleTrack | null) => void;
  spans: PlaybackSpans;
}) {
  let position = 0;
  let paused = false;
  let stopped = false;
  let tracks: Tracks = { audioTrackId, subtitleTrackId };
  let timer: ReturnType<typeof setInterval> | undefined;
  const end = () => {
    stopped = true;
    clearInterval(timer);
  };
  const report = () => {
    const sent = { position, paused, stopped, ...tracks };
    sendReport(sent).then(
      (response) => {
        if (
          !sent.stopped &&
          !stopped &&
          response.status === 404 &&
          response.data.type === "urn:selfless:problem:playback-ended"
        ) {
          end();
          restart({ position, paused, forceHls, tracks });
        }
      },
      () => undefined,
    );
  };
  const switchStream = (switched: Tracks, throughHls: boolean) => {
    end();
    spans.switched();
    restart({ position, paused, forceHls: forceHls || throughHls, tracks: switched });
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
          if (method === "directPlay") {
            end();
            spans.switched(event.errorType);
            restart({ position, paused, forceHls: true, tracks });
          } else {
            spans.failed(event.errorType);
            showError();
          }
          break;
        case "stopped":
          end();
          spans.stopped();
          report();
          break;
      }
    },
    selected() {
      return tracks;
    },
    applySelectedAudio() {
      const playing = audioTracks.find((track) => track.default) ?? audioTracks[0];
      const selected = audioTracks.find((track) => track.id === audioTrackId);
      if (stopped || method !== "directPlay" || !selected || selected === playing) {
        return;
      }
      if (audioSwitchesInPlace) {
        switchAudio(selected);
      } else {
        switchStream(tracks, true);
      }
    },
    selectAudio(track: PlaybackAudioTrack) {
      if (stopped || track.id === tracks.audioTrackId) {
        return;
      }
      if (track.inStream && audioSwitchesInPlace) {
        tracks = { ...tracks, audioTrackId: track.id };
        switchAudio(track);
      } else {
        switchStream({ ...tracks, audioTrackId: track.id }, track.inStream);
      }
    },
    selectSubtitle(track: PlaybackSubtitleTrack | null) {
      if (stopped || (track?.id ?? null) === tracks.subtitleTrackId) {
        return;
      }
      if (track === null || track.inStream) {
        tracks = { ...tracks, subtitleTrackId: track?.id ?? null };
        switchSubtitle(track);
      } else {
        switchStream({ ...tracks, subtitleTrackId: track.id }, false);
      }
    },
  };
}

function canonicalLanguage(language: string | null) {
  try {
    return language ? Intl.getCanonicalLocales(language.replaceAll("_", "-"))[0] : undefined;
  } catch {
    return undefined;
  }
}

export function matchTrack<T extends PlaybackAudioTrack | PlaybackSubtitleTrack>(
  track: PlayerTrack,
  playerTracks: PlayerTrack[],
  tracks: T[],
) {
  const language = canonicalLanguage(track.language);
  if (language === undefined) {
    return undefined;
  }
  const peers = playerTracks.filter((x) => canonicalLanguage(x.language) === language);
  const candidates = tracks.filter((x) => x.inStream && canonicalLanguage(x.language) === language);
  return candidates[peers.findIndex((x) => x.label === track.label)] as T | undefined;
}

export function playerTrackFor<P extends PlayerTrack, T extends PlaybackAudioTrack | PlaybackSubtitleTrack>(
  track: T,
  playerTracks: P[],
  tracks: T[],
) {
  const language = canonicalLanguage(track.language);
  if (language === undefined) {
    return undefined;
  }
  const peers = playerTracks.filter((x) => canonicalLanguage(x.language) === language);
  const candidates = tracks.filter((x) => x.inStream && canonicalLanguage(x.language) === language);
  return peers[candidates.indexOf(track)];
}
