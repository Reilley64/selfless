import type { PlaybackNextEpisode } from "@selfless/api-client";
import { useEffect, useRef } from "react";
import { Platform, type View as HostView } from "react-native";
import { Pressable, Text, View } from "react-native-css/components";
import { episodeLabel } from "./episodeLabel";
import type { FocusGuideInstance } from "./FocusGuide";
import { ArtworkImage } from "./Tile";

export function NextEpisodeButton({
  episode,
  countdown,
  onPress,
}: {
  episode: PlaybackNextEpisode;
  countdown: number | null;
  onPress: () => void;
}) {
  const button = useRef<HostView>(null);
  const focus = () => (button.current as unknown as FocusGuideInstance | null)?.requestTVFocus();
  useEffect(() => {
    if (Platform.isTV) {
      focus();
    }
  }, []);
  return (
    <Pressable
      ref={button}
      role="button"
      hasTVPreferredFocus
      onBlur={Platform.isTV ? focus : undefined}
      onPress={onPress}
      className={`group absolute flex-row items-center rounded-medium bg-background/80 hover:bg-button-focus focus:bg-button-focus ${Platform.isTV ? "right-16 bottom-16 gap-5 p-4" : "right-6 bottom-16 gap-3 p-2"}`}
    >
      {episode.thumb && (
        <ArtworkImage
          testID="next-episode-thumb"
          artwork={episode.thumb}
          resizeMode="cover"
          className={`rounded-small bg-surface ${Platform.isTV ? "h-[135px] w-[240px]" : "h-[72px] w-[128px]"}`}
        />
      )}
      <View className={Platform.isTV ? "gap-1 pr-4" : "pr-2"}>
        <Text
          className={`font-sans font-bold text-text-primary group-hover:text-text-inverse group-focus:text-text-inverse ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}
        >
          {countdown === null ? "Next episode" : `Next episode in ${countdown}`}
        </Text>
        <Text
          className={`font-sans text-text-secondary group-hover:text-text-inverse group-focus:text-text-inverse ${Platform.isTV ? "text-tv-meta" : "text-web-meta"}`}
        >
          {`${episodeLabel(episode.seasonNumber, episode.episodeNumber)} · ${episode.title}`}
        </Text>
      </View>
    </Pressable>
  );
}
