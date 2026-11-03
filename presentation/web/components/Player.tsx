import { type Playback, reportPlaybackProgress, startPlayback } from "@selfless/api-client";
import { createPlayerLogic, type Resume } from "@selfless/ui";
import { type Href, useRouter } from "expo-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Pressable, Text, View } from "react-native-css/components";
import { capabilityProfile } from "../capabilityProfile";
import { ServerUnreachable } from "./form";
import { NotFound } from "./NotFound";

type Start = Awaited<ReturnType<typeof startPlayback>> | "unreachable";

type PlayerProps = { mediaFileId: string; startPosition: number; backHref: Href };

export function Player(props: PlayerProps) {
  return <Playing key={`${props.mediaFileId}@${props.startPosition}`} {...props} />;
}

function Playing({ mediaFileId, startPosition, backHref }: PlayerProps) {
  const [start, setStart] = useState<Start>();
  const [resume, setResume] = useState<Resume>({ position: startPosition, paused: false });
  useEffect(() => {
    let current = true;
    capabilityProfile()
      .then((profile) => startPlayback({ mediaFileId, startPosition: resume.position, profile }))
      .then(
        (response) => current && setStart(response),
        () => current && setStart("unreachable"),
      );
    return () => {
      current = false;
    };
  }, [mediaFileId, resume]);
  if (start === undefined) {
    return <Screen backHref={backHref} />;
  }
  if (start === "unreachable") {
    return (
      <Screen backHref={backHref}>
        <ServerUnreachable />
      </Screen>
    );
  }
  switch (start.status) {
    case 201:
      return (
        <Screen backHref={backHref}>
          <Video key={start.data.playbackId} playback={start.data} resume={resume} restart={setResume} />
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
  resume,
  restart,
}: {
  playback: Playback;
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
      }),
    [playback.playbackId, restart],
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
      onTimeUpdate={(event) => logic.handle({ type: "position", position: at(event.currentTarget) })}
      onEnded={(event) => logic.handle({ type: "stopped", position: at(event.currentTarget) })}
      onError={() => setFailed(true)}
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
