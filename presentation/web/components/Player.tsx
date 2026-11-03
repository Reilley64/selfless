import { type Playback, reportPlaybackProgress, startPlayback } from "@selfless/api-client";
import { createPlayerLogic } from "@selfless/ui";
import { type Href, useRouter } from "expo-router";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Pressable, Text, View } from "react-native-css/components";
import { capabilityProfile } from "../capabilityProfile";
import { ServerUnreachable } from "./form";
import { NotFound } from "./NotFound";

type Start = Awaited<ReturnType<typeof startPlayback>> | "unreachable";

export function Player({
  mediaFileId,
  startPosition,
  backHref,
}: {
  mediaFileId: string;
  startPosition: number;
  backHref: Href;
}) {
  const [start, setStart] = useState<Start>();
  useEffect(() => {
    let current = true;
    capabilityProfile()
      .then((profile) => startPlayback({ mediaFileId, startPosition, profile }))
      .then(
        (response) => current && setStart(response),
        () => current && setStart("unreachable"),
      );
    return () => {
      current = false;
    };
  }, [mediaFileId, startPosition]);
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
          <Video playback={start.data} startPosition={startPosition} />
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

function Video({ playback, startPosition }: { playback: Playback; startPosition: number }) {
  const [failed, setFailed] = useState(false);
  const position = useRef(startPosition);
  const logic = useMemo(
    () =>
      createPlayerLogic((report) => {
        reportPlaybackProgress(playback.playbackId, report).catch(() => undefined);
      }),
    [playback.playbackId],
  );
  useEffect(() => () => logic.handle({ type: "stopped", position: position.current }), [logic]);
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
      autoPlay
      onLoadedMetadata={(event) => {
        event.currentTarget.currentTime = startPosition;
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
