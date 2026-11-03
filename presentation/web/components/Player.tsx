import {
  type Playback,
  type PlaybackAudioTrack,
  type PlaybackSubtitleTrack,
  reportPlaybackProgress,
  startPlayback,
  useGetMe,
} from "@selfless/api-client";
import { createPlayerLogic, type PlaybackSpans, type Resume, startPlaybackSpans, type Tracks } from "@selfless/ui";
import { getParser } from "bowser";
import { type Href, useRouter } from "expo-router";
import HlsJs, { Events } from "hls.js";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";
import { capabilityProfile } from "../capabilityProfile";
import { ServerUnreachable } from "./form";
import { NotFound } from "./NotFound";

const mediaErrors: Record<number, string> = {
  1: "MEDIA_ERR_ABORTED",
  2: "MEDIA_ERR_NETWORK",
  3: "MEDIA_ERR_DECODE",
  4: "MEDIA_ERR_SRC_NOT_SUPPORTED",
};

const hlsJs = HlsJs.isSupported() && getParser(navigator.userAgent).getBrowserName(true) !== "safari";

type AudioTrackList = ArrayLike<{ enabled: boolean }> & Partial<Pick<EventTarget, "addEventListener" | "removeEventListener">>;

function audioTracksOf(element: HTMLVideoElement | undefined): AudioTrackList {
  return (element as (HTMLVideoElement & { audioTracks?: AudioTrackList }) | undefined)?.audioTracks ?? [];
}

const languageNames = new Intl.DisplayNames([navigator.language], { type: "language" });

type Start = { response: Awaited<ReturnType<typeof startPlayback>>; spans: PlaybackSpans } | "unreachable";

type PlayerProps = { mediaFileId: string; startPosition: number; backHref: Href };

export function Player(props: PlayerProps) {
  return <Playing key={`${props.mediaFileId}@${props.startPosition}`} {...props} />;
}

function Playing({ mediaFileId, startPosition, backHref }: PlayerProps) {
  const { data: me, isError } = useGetMe();
  const userId = me?.status === 200 ? me.data.id : undefined;
  const [start, setStart] = useState<Start>();
  const [resume, setResume] = useState<Resume>({ position: startPosition, paused: false, forceHls: false });
  useEffect(() => {
    if (userId === undefined) {
      return;
    }
    let current = true;
    let answered = false;
    const spans = startPlaybackSpans(userId, mediaFileId);
    capabilityProfile()
      .then((profile) =>
        spans.during(() =>
          startPlayback({
            mediaFileId,
            startPosition: resume.position,
            profile,
            forceHls: resume.forceHls,
            ...(resume.tracks && {
              audioTrackId: resume.tracks.audioTrackId ?? undefined,
              subtitleTrackId: resume.tracks.subtitleTrackId,
            }),
          }),
        ),
      )
      .then(
        (response) => {
          answered = true;
          if (response.status !== 201) {
            spans.failed(String(response.status));
          }
          if (current) {
            setStart({ response, spans });
          }
        },
        (error: unknown) => {
          answered = true;
          spans.failed(error instanceof Error ? error.name : "_OTHER");
          if (current) {
            setStart("unreachable");
          }
        },
      );
    return () => {
      current = false;
      if (!answered) {
        spans.stopped();
      }
    };
  }, [userId, mediaFileId, resume]);
  if (isError || start === "unreachable") {
    return (
      <Screen backHref={backHref}>
        <ServerUnreachable />
      </Screen>
    );
  }
  if (start === undefined) {
    return <Screen backHref={backHref} />;
  }
  switch (start.response.status) {
    case 201:
      return (
        <Screen backHref={backHref}>
          <Video
            key={start.response.data.playbackId}
            playback={start.response.data}
            spans={start.spans}
            resume={resume}
            restart={setResume}
          />
        </Screen>
      );
    case 404:
      return <NotFound />;
    case 422:
      return <Message backHref={backHref}>selfless can&apos;t read this file.</Message>;
    case 501:
      return <Message backHref={backHref}>This browser can&apos;t play this file yet.</Message>;
    default:
      return (
        <Screen backHref={backHref}>
          <ServerUnreachable />
        </Screen>
      );
  }
}

function Video({
  playback,
  spans,
  resume,
  restart,
}: {
  playback: Playback;
  spans: PlaybackSpans;
  resume: Resume;
  restart: (resume: Resume) => void;
}) {
  const [failed, setFailed] = useState(false);
  const [selected, setSelected] = useState<Tracks>({
    audioTrackId: playback.audioTrackId,
    subtitleTrackId: playback.subtitleTrackId,
  });
  const position = useRef(resume.position);
  const native = playback.method === "directPlay" || !hlsJs;
  const logic = useMemo(() => {
    const media: { video?: HTMLVideoElement; hls?: HlsJs } = {};
    const streamSubtitles = playback.subtitleTracks.filter((track) => track.inStream);
    const showSubtitle = (track: PlaybackSubtitleTrack | null) => {
      const index = track ? streamSubtitles.indexOf(track) : -1;
      if (media.hls) {
        media.hls.subtitleTrack = index;
        return;
      }
      const textTracks = Array.from(media.video?.textTracks ?? []).filter((x) => x.kind !== "metadata");
      textTracks.forEach((x, i) => {
        x.mode = i === index ? "showing" : "disabled";
      });
    };
    const showAudio = (track: PlaybackAudioTrack) => {
      const index = playback.audioTracks.indexOf(track);
      if (media.hls) {
        media.hls.audioTrack = index;
        return;
      }
      Array.from(audioTracksOf(media.video)).forEach((x, i) => {
        x.enabled = i === index;
      });
    };
    return {
      attach: (attached: typeof media) => Object.assign(media, attached),
      video: () => media.video,
      showSubtitle,
      showAudio,
      ...createPlayerLogic({
        playback,
        audioSwitchesInPlace: playback.method !== "directPlay" || "audioTracks" in HTMLMediaElement.prototype,
        forceHls: resume.forceHls,
        sendReport: (report) => reportPlaybackProgress(playback.playbackId, report, { keepalive: true }),
        restart,
        showError: () => setFailed(true),
        switchAudio: (track) => {
          showAudio(track);
          setSelected((current) => ({ ...current, audioTrackId: track.id }));
        },
        switchSubtitle: (track) => {
          showSubtitle(track);
          setSelected((current) => ({ ...current, subtitleTrackId: track?.id ?? null }));
        },
        spans,
      }),
    };
  }, [playback, resume.forceHls, restart, spans]);
  useEffect(() => {
    const stop = () => logic.handle({ type: "stopped", position: position.current });
    const closed = (event: PageTransitionEvent) => {
      if (!event.persisted) {
        stop();
      }
    };
    window.addEventListener("pagehide", closed);
    return () => {
      window.removeEventListener("pagehide", closed);
      stop();
    };
  }, [logic]);
  useEffect(() => {
    const element = logic.video();
    if (!element || !native) {
      return;
    }
    const showSelected = () => {
      const { audioTrackId, subtitleTrackId } = logic.selected();
      const audio = playback.audioTracks.find((track) => track.id === audioTrackId);
      if (audio) {
        logic.showAudio(audio);
      }
      logic.showSubtitle(playback.subtitleTracks.find((track) => track.id === subtitleTrackId) ?? null);
    };
    const audioTracks = audioTracksOf(element);
    element.textTracks.addEventListener("addtrack", showSelected);
    audioTracks.addEventListener?.("addtrack", showSelected);
    return () => {
      element.textTracks.removeEventListener("addtrack", showSelected);
      audioTracks.removeEventListener?.("addtrack", showSelected);
    };
  }, [logic, native, playback]);
  useEffect(() => {
    const element = logic.video();
    if (!element || native) {
      return;
    }
    const player = new HlsJs({ startPosition: resume.position });
    logic.attach({ hls: player });
    player.on(Events.ERROR, (_, data) => {
      if (data.fatal) {
        logic.handle({ type: "failed", position: element.currentTime, errorType: data.details });
      }
    });
    player.loadSource(playback.url);
    player.attachMedia(element);
    return () => {
      logic.attach({ hls: undefined });
      player.destroy();
    };
  }, [logic, native, playback, resume.position]);
  const at = (element: HTMLVideoElement) => {
    position.current = element.currentTime;
    return element.currentTime;
  };
  if (failed) {
    return <Text className="font-sans text-web-body text-accent">This browser can&apos;t play this file.</Text>;
  }
  return (
    <>
      <video
        ref={(element) => {
          logic.attach({ video: element ?? undefined });
        }}
        className="h-full w-full bg-black"
        src={native ? playback.url : undefined}
        controls
        autoPlay={!resume.paused}
        onLoadedMetadata={(event) => {
          if (native) {
            event.currentTarget.currentTime = resume.position;
            logic.applySelectedAudio();
          }
        }}
        onPlaying={(event) => logic.handle({ type: "playing", position: at(event.currentTarget) })}
        onPause={(event) => {
          if (!event.currentTarget.ended) {
            logic.handle({ type: "paused", position: at(event.currentTarget) });
          }
        }}
        onSeeked={(event) => logic.handle({ type: "seeked", position: at(event.currentTarget) })}
        onTimeUpdate={(event) => logic.handle({ type: "position", position: at(event.currentTarget) })}
        onEnded={(event) => logic.handle({ type: "stopped", position: at(event.currentTarget) })}
        onError={(event) =>
          logic.handle({
            type: "failed",
            position: at(event.currentTarget),
            errorType: mediaErrors[event.currentTarget.error?.code ?? 0] ?? "_OTHER",
          })
        }
      />
      <TrackMenus
        playback={playback}
        selected={selected}
        onAudio={logic.selectAudio}
        onSubtitle={logic.selectSubtitle}
      />
    </>
  );
}

function languageName(code: string | null) {
  if (code === null) {
    return "Unknown";
  }
  try {
    return languageNames.of(code) ?? code;
  } catch {
    return code;
  }
}

function trackName(track: PlaybackAudioTrack | PlaybackSubtitleTrack) {
  const language = languageName(track.language);
  const marks = [
    track.label !== null && track.label.toLowerCase() !== language.toLowerCase() ? track.label : undefined,
    "hearingImpaired" in track && track.hearingImpaired ? "SDH" : undefined,
    "forced" in track && track.forced ? "Forced" : undefined,
  ].filter((mark) => mark !== undefined);
  return marks.length === 0 ? language : `${language} (${marks.join(", ")})`;
}

function TrackMenus({
  playback,
  selected,
  onAudio,
  onSubtitle,
}: {
  playback: Playback;
  selected: Tracks;
  onAudio: (track: PlaybackAudioTrack) => void;
  onSubtitle: (track: PlaybackSubtitleTrack | null) => void;
}) {
  const [open, setOpen] = useState<"Audio" | "Subtitles">();
  const subtitles = playback.subtitleTracks.filter((track) => track.text);
  const menus = [
    {
      label: "Audio" as const,
      items: playback.audioTracks.map((track) => ({
        key: track.id,
        name: trackName(track),
        checked: track.id === selected.audioTrackId,
        choose: () => onAudio(track),
      })),
    },
    {
      label: "Subtitles" as const,
      items: [
        { key: "off", name: "Off", checked: selected.subtitleTrackId === null, choose: () => onSubtitle(null) },
        ...subtitles.map((track) => ({
          key: track.id,
          name: trackName(track),
          checked: track.id === selected.subtitleTrackId,
          choose: () => onSubtitle(track),
        })),
      ],
    },
  ].filter((menu) => (menu.label === "Audio" ? playback.audioTracks.length > 0 : subtitles.length > 0));
  return (
    <View className="absolute top-4 right-4 z-50 flex-row gap-2">
      {menus.map((menu) => (
        <View key={menu.label} className="relative">
          <Pressable
            role="button"
            aria-expanded={open === menu.label}
            className="h-11 justify-center rounded-pill bg-background/60 px-4 hover:bg-surface-raised"
            onPress={() => setOpen(open === menu.label ? undefined : menu.label)}
          >
            <Text className="font-sans font-semibold text-web-meta text-text-primary">{menu.label}</Text>
          </Pressable>
          {open === menu.label && (
            <ScrollView
              role="menu"
              aria-label={menu.label}
              className="absolute top-12 right-0 max-h-[420px] w-[280px] rounded-medium border border-surface-raised bg-surface"
              contentContainerClassName="p-2"
            >
              {menu.items.map((item) => (
                <Pressable
                  key={item.key}
                  role="menuitem"
                  aria-checked={item.checked}
                  className="flex-row gap-2 rounded-small p-2 hover:bg-surface-raised"
                  onPress={() => {
                    setOpen(undefined);
                    item.choose();
                  }}
                >
                  <Text className="w-4 font-sans text-web-meta text-accent">{item.checked ? "✓" : ""}</Text>
                  <Text className="flex-1 font-sans text-web-meta text-text-primary">{item.name}</Text>
                </Pressable>
              ))}
            </ScrollView>
          )}
        </View>
      ))}
    </View>
  );
}

function Message({ backHref, children }: { backHref: Href; children: ReactNode }) {
  return (
    <Screen backHref={backHref}>
      <View className="flex-1 items-center justify-center">
        <Text className="font-sans text-web-body text-text-primary">{children}</Text>
      </View>
    </Screen>
  );
}

function Screen({ backHref, children }: { backHref: Href; children?: ReactNode }) {
  const router = useRouter();
  const back = () => (router.canGoBack() ? router.back() : router.replace(backHref));
  return (
    <View className="flex-1 bg-black">
      {children}
      <Pressable
        role="button"
        aria-label="Back"
        className="absolute top-4 left-4 h-11 w-11 items-center justify-center rounded-full bg-background/60 hover:bg-button-focus"
        onPress={back}
      >
        <Text className="font-sans font-bold text-web-heading text-text-primary">←</Text>
      </Pressable>
    </View>
  );
}
