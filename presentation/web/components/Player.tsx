import { type Playback, reportPlaybackProgress, startPlayback, useGetMe } from "@selfless/api-client";
import { createPlayerLogic, type PlaybackSpans, type Resume, startPlaybackSpans } from "@selfless/ui";
import { type Href, useRouter } from "expo-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Pressable, Text, View } from "react-native-css/components";
import { capabilityProfile } from "../capabilityProfile";
import { ServerUnreachable } from "./form";
import { NotFound } from "./NotFound";

const mediaErrors: Record<number, string> = {
  1: "MEDIA_ERR_ABORTED",
  2: "MEDIA_ERR_NETWORK",
  3: "MEDIA_ERR_DECODE",
  4: "MEDIA_ERR_SRC_NOT_SUPPORTED",
};

type Start = { response: Awaited<ReturnType<typeof startPlayback>>; spans: PlaybackSpans } | "unreachable";

type PlayerProps = { mediaFileId: string; startPosition: number; backHref: Href };

export function Player(props: PlayerProps) {
  return <Playing key={`${props.mediaFileId}@${props.startPosition}`} {...props} />;
}

function Playing({ mediaFileId, startPosition, backHref }: PlayerProps) {
  const { data: me, isError } = useGetMe();
  const userId = me?.status === 200 ? me.data.id : undefined;
  const [start, setStart] = useState<Start>();
  const [resume, setResume] = useState<Resume>({ position: startPosition, paused: false });
  useEffect(() => {
    if (userId === undefined) {
      return;
    }
    let current = true;
    let answered = false;
    const spans = startPlaybackSpans(userId, mediaFileId);
    capabilityProfile()
      .then((profile) => spans.during(() => startPlayback({ mediaFileId, startPosition: resume.position, profile })))
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
  const position = useRef(resume.position);
  const logic = useMemo(
    () =>
      createPlayerLogic({
        sendReport: (report) => reportPlaybackProgress(playback.playbackId, report, { keepalive: true }),
        restart,
        spans,
      }),
    [playback.playbackId, restart, spans],
  );
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
  const at = (video: HTMLVideoElement) => {
    position.current = video.currentTime;
    return video.currentTime;
  };
  if (failed) {
    return <Text className="font-sans text-web-body text-accent">This browser can&apos;t play this file.</Text>;
  }
  return (
    <video
      className="h-full w-full bg-black"
      src={playback.url}
      controls
      autoPlay={!resume.paused}
      onLoadedMetadata={(event) => {
        event.currentTarget.currentTime = resume.position;
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
      onError={(event) => {
        logic.handle({
          type: "failed",
          position: at(event.currentTarget),
          errorType: mediaErrors[event.currentTarget.error?.code ?? 0] ?? "_OTHER",
        });
        setFailed(true);
      }}
    />
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
