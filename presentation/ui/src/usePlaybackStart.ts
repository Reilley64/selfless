import { type CapabilityProfile, startPlayback, useGetMe } from "@selfless/api-client";
import { useEffect, useState } from "react";
import { type PlaybackSpans, type Resume, startPlaybackSpans } from "./playerLogic";

export type PlaybackStart = { response: Awaited<ReturnType<typeof startPlayback>>; spans: PlaybackSpans } | "unreachable";

export function usePlaybackStart({
  mediaFileId,
  startPosition,
  autoplayed,
  stillWatching,
  profile,
}: {
  mediaFileId: string;
  startPosition: number;
  autoplayed: number;
  stillWatching: boolean;
  profile: () => Promise<CapabilityProfile>;
}) {
  const { data: me, isError } = useGetMe();
  const userId = me?.status === 200 ? me.data.id : undefined;
  const [start, setStart] = useState<PlaybackStart>();
  const [resume, setResume] = useState<Resume>({
    position: startPosition,
    paused: stillWatching,
    forceHls: false,
    autoplayed,
    stillWatching,
  });
  useEffect(() => {
    if (userId === undefined) {
      return;
    }
    let current = true;
    let answered = false;
    const spans = startPlaybackSpans(userId, mediaFileId);
    profile()
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
  }, [userId, mediaFileId, resume, profile]);
  return { start: isError ? ("unreachable" as const) : start, resume, restart: setResume };
}
