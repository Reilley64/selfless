import { type Attributes, ROOT_CONTEXT, type Span, SpanStatusCode, context, trace } from "@opentelemetry/api";
import { ATTR_ERROR_TYPE } from "@opentelemetry/semantic-conventions";
import type {
  Playback,
  PlaybackAudioTrack,
  PlaybackNextEpisode,
  PlaybackSubtitleTrack,
  reportPlaybackProgress,
} from "@selfless/api-client";
import { SELFLESS_MEDIA_FILE_ID } from "./telemetry/attributes";
import {
  SELFLESS_PLAYBACK_FAILED,
  SELFLESS_PLAYBACK_PAUSED,
  SELFLESS_PLAYBACK_SEEKED,
  SELFLESS_PLAYBACK_STARTED,
  SELFLESS_PLAYBACK_STILL_WATCHING_PROMPT,
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
  | { type: "stopped"; position: number }
  | { type: "ended"; position: number };

export type Tracks = { audioTrackId: string | null; subtitleTrackId: string | null };

export type ProgressReport = { position: number; paused: boolean; stopped: boolean } & Tracks;

export type EpisodeStart = { autoplayed: number; stillWatching: boolean };

export type Resume = { position: number; paused: boolean; forceHls: boolean; tracks?: Tracks } & EpisodeStart;

export type PlaybackSpans = ReturnType<typeof startPlaybackSpans>;

export type PlayerTrack = { language: string; label: string };

export type NextEpisodeButtonState = { countdown: number | null };

export const reportInterval = 10_000;

const nextEpisodeLead = 5;

const autoplaysBeforeAsking = 3;

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
    stillWatchingPrompt: end(SELFLESS_PLAYBACK_STILL_WATCHING_PROMPT),
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
  playback: {
    method,
    duration,
    audioTracks,
    audioTrackId,
    subtitleTrackId,
    nextEpisode,
    autoplayNextEpisode,
    askIfStillWatching,
  },
  resume,
  audioSwitchesInPlace,
  sendReport,
  restart,
  showError,
  switchAudio,
  switchSubtitle,
  showNextEpisode,
  playNextEpisode,
  returnToTitle,
  showStillWatching,
  play,
  spans,
}: {
  playback: Pick<
    Playback,
    | "method"
    | "duration"
    | "audioTracks"
    | "audioTrackId"
    | "subtitleTrackId"
    | "nextEpisode"
    | "autoplayNextEpisode"
    | "askIfStillWatching"
  >;
  resume: Resume;
  audioSwitchesInPlace: boolean;
  sendReport: (report: ProgressReport) => ReturnType<typeof reportPlaybackProgress>;
  restart: (resume: Resume) => void;
  showError: () => void;
  switchAudio: (track: PlaybackAudioTrack) => void;
  switchSubtitle: (track: PlaybackSubtitleTrack | null) => void;
  showNextEpisode: (button: NextEpisodeButtonState | null) => void;
  playNextEpisode: (episode: PlaybackNextEpisode, start: EpisodeStart) => void;
  returnToTitle: () => void;
  showStillWatching: (show: boolean) => void;
  play: () => void;
  spans: PlaybackSpans;
}) {
  const { forceHls } = resume;
  let { paused, autoplayed, stillWatching } = resume;
  let position = 0;
  let stopped = false;
  let tracks: Tracks = { audioTrackId, subtitleTrackId };
  let timer: ReturnType<typeof setInterval> | undefined;
  let button: NextEpisodeButtonState | null = null;
  const updateButton = () => {
    const left = duration - position;
    const next =
      nextEpisode === null || stopped || left > nextEpisodeLead
        ? null
        : { countdown: autoplayNextEpisode ? Math.max(1, Math.ceil(left)) : null };
    if (next?.countdown !== button?.countdown) {
      button = next;
      showNextEpisode(next);
    }
  };
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
          restartAt({});
        }
      },
      () => undefined,
    );
  };
  const stop = () => {
    end();
    spans.stopped();
    report();
    updateButton();
  };
  const restartAt = (changes: Partial<Resume>) =>
    restart({ position, paused, forceHls, tracks, autoplayed, stillWatching, ...changes });
  const switchStream = (switched: Tracks, throughHls: boolean) => {
    end();
    spans.switched();
    restartAt({ forceHls: forceHls || throughHls, tracks: switched });
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
          if (paused) {
            autoplayed = 0;
          }
          paused = false;
          if (stillWatching) {
            stillWatching = false;
            showStillWatching(false);
          }
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
            autoplayed = 0;
            spans.seeked();
          }
          break;
        case "failed":
          if (method === "directPlay") {
            end();
            spans.switched(event.errorType);
            restartAt({ forceHls: true });
          } else {
            spans.failed(event.errorType);
            showError();
          }
          break;
        case "stopped":
          stop();
          break;
        case "ended":
          stop();
          if (nextEpisode === null || !autoplayNextEpisode) {
            returnToTitle();
          } else if (askIfStillWatching && autoplayed >= autoplaysBeforeAsking) {
            spans.stillWatchingPrompt();
            playNextEpisode(nextEpisode, { autoplayed: 0, stillWatching: true });
          } else {
            playNextEpisode(nextEpisode, { autoplayed: autoplayed + 1, stillWatching: false });
          }
          break;
      }
      updateButton();
    },
    playNextEpisode() {
      if (stopped || nextEpisode === null) {
        return;
      }
      stop();
      playNextEpisode(nextEpisode, { autoplayed: 0, stillWatching: false });
    },
    keepWatching() {
      stillWatching = false;
      showStillWatching(false);
      play();
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
      autoplayed = 0;
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
      autoplayed = 0;
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
