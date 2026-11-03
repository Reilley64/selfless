import { type Playback, reportPlaybackProgress, serverUrl, startPlayback, useGetMe } from "@selfless/api-client";
import {
  ActionButton,
  createPlayerLogic,
  episodeLabel,
  FocusGuide,
  type PlaybackSpans,
  type Resume,
  serverUnreachable,
  startPlaybackSpans,
} from "@selfless/ui";
import { useRouter } from "expo-router";
import { useVideoPlayer, VideoView } from "expo-video";
import { useEffect, useMemo, useRef, useState } from "react";
import { StyleSheet } from "react-native";
import { Text, View } from "react-native-css/components";
import { appleTvProfile } from "../appleTvProfile";
import { useFirstFieldFocus } from "./form";
import { NotFound } from "./NotFound";

type Start = { response: Awaited<ReturnType<typeof startPlayback>>; spans: PlaybackSpans } | "unreachable";

type PlayerProps = { mediaFileId: string; startPosition: number };

export function Player(props: PlayerProps) {
  return <Playing key={`${props.mediaFileId}@${props.startPosition}`} {...props} />;
}

function Playing({ mediaFileId, startPosition }: PlayerProps) {
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
    spans
      .during(() =>
        startPlayback({ mediaFileId, startPosition: resume.position, profile: appleTvProfile, forceHls: resume.forceHls }),
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
  const retry = () => setResume({ ...resume });
  if (isError || start === "unreachable") {
    return <Failure message={serverUnreachable} onRetry={retry} />;
  }
  if (start === undefined) {
    return <View className="flex-1 bg-black" />;
  }
  switch (start.response.status) {
    case 201:
      return (
        <Video
          key={start.response.data.playbackId}
          playback={start.response.data}
          spans={start.spans}
          resume={resume}
          restart={setResume}
        />
      );
    case 404:
      return <NotFound />;
    case 422:
      return <Failure message="selfless can't read this file." />;
    case 501:
      return <Failure message="This TV can't play this file yet." />;
    default:
      return <Failure message={serverUnreachable} onRetry={retry} />;
  }
}

function title(playback: Playback) {
  const [first] = playback.episodes;
  if (first === undefined) {
    return playback.movie?.title;
  }
  const labels = playback.episodes.map((x) => episodeLabel(x.seasonNumber, x.episodeNumber)).join(" & ");
  const titles = playback.episodes.map((x) => x.title).join(" & ");
  return `${first.showTitle} · ${labels} · ${titles}`;
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
        method: playback.method,
        forceHls: resume.forceHls,
        sendReport: (report) => reportPlaybackProgress(playback.playbackId, report),
        restart,
        showError: () => setFailed(true),
        spans,
      }),
    [playback.playbackId, playback.method, resume.forceHls, restart, spans],
  );
  const player = useVideoPlayer({ uri: serverUrl(playback.url), metadata: { title: title(playback) } }, (player) => {
    player.timeUpdateEventInterval = 1;
    player.currentTime = resume.position;
    if (!resume.paused) {
      player.play();
    }
  });
  useEffect(() => {
    const subscriptions = [
      player.addListener("playingChange", ({ isPlaying }) => {
        if (isPlaying) {
          logic.handle({ type: "playing", position: position.current });
        } else if (player.status === "readyToPlay") {
          logic.handle({ type: "paused", position: position.current });
        }
      }),
      player.addListener("timeUpdate", ({ currentTime }) => {
        position.current = currentTime;
        logic.handle({ type: "position", position: currentTime });
      }),
      player.addListener("playToEnd", () => logic.handle({ type: "stopped", position: position.current })),
      player.addListener("statusChange", ({ status }) => {
        if (status === "error") {
          logic.handle({ type: "failed", position: position.current, errorType: "_OTHER" });
        }
      }),
    ];
    return () => {
      for (const subscription of subscriptions) {
        subscription.remove();
      }
      logic.handle({ type: "stopped", position: position.current });
    };
  }, [logic, player]);
  if (failed) {
    return (
      <Failure
        message="This file stopped playing."
        onRetry={() => restart({ position: position.current, paused: false, forceHls: resume.forceHls })}
      />
    );
  }
  return <VideoView player={player} nativeControls contentFit="contain" style={StyleSheet.absoluteFill} />;
}

function Failure({ message, onRetry }: { message: string; onRetry?: () => void }) {
  const router = useRouter();
  const guide = useFirstFieldFocus();
  return (
    <FocusGuide autoFocus ref={guide} className="flex-1 items-start justify-center gap-8 bg-background px-tv-inset">
      <Text className="font-sans text-tv-body text-text-primary">{message}</Text>
      <View className="flex-row gap-3">
        {onRetry && <ActionButton label="Retry" primary onPress={onRetry} />}
        <ActionButton label="Back" onPress={() => router.back()} />
      </View>
    </FocusGuide>
  );
}
