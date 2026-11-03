import { useListSeasonEpisodes } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { Platform } from "react-native";
import { Pressable, Text, View } from "react-native-css/components";
import { playHref } from "./playHref";
import { ProgressBar } from "./ProgressBar";
import { runtimeLabel } from "./runtimeLabel";
import { ArtworkImage } from "./Tile";
import { WatchedMark } from "./WatchedMark";

export function EpisodeList({ seasonId }: { seasonId: string }) {
  const router = useRouter();
  const { data } = useListSeasonEpisodes(seasonId);
  const episodes = data?.status === 200 ? data.data.episodes : [];
  const tv = Platform.isTV;
  return (
    <View className={tv ? "flex-1 gap-6" : "max-w-[1100px] px-web-inset"}>
      {episodes.map((episode) => (
        <View
          key={episode.id}
          className={tv ? "" : "flex-row items-center gap-2 border-surface-raised border-b"}
        >
          <Pressable
            className={
              tv
                ? "flex-1 flex-row items-center gap-8 rounded-large p-3 focus:bg-surface-raised"
                : "group flex-1 flex-row items-center gap-5 rounded-large p-4 hover:bg-surface-raised"
            }
            aria-label={`Play E${episode.number} ${episode.title}`}
            onPress={() => router.push(playHref({ episodeId: episode.id }))}
            onLongPress={tv ? () => router.push(`/episodes/${episode.id}`) : undefined}
          >
            {({ focused }) => (
              <>
                <Text
                  className={`text-center font-black font-sans text-text-muted ${tv ? "w-14 text-tv-heading" : "w-10 text-web-heading"}`}
                >
                  {episode.number}
                </Text>
                <View
                  className={`overflow-hidden rounded-small bg-surface ${tv ? "h-[148px] w-[264px]" : "h-[117px] w-[208px]"}`}
                >
                  {episode.thumb && (
                    <ArtworkImage
                      artwork={episode.thumb}
                      testID="episode-thumb"
                      className="h-full w-full"
                      resizeMode="cover"
                    />
                  )}
                  {episode.progress != null && (
                    <ProgressBar progress={episode.progress} className="absolute right-2 bottom-2 left-2 h-1" />
                  )}
                  {episode.watched && <WatchedMark className="absolute top-2 right-2" />}
                </View>
                <View className="flex-1 gap-1">
                  <View className="flex-row items-center gap-3">
                    <Text
                      className={`shrink font-bold font-sans text-text-primary ${tv ? "text-tv-row" : "text-web-body"}`}
                    >
                      {episode.title}
                    </Text>
                    {episode.runtimeMinutes != null && episode.runtimeMinutes > 0 && (
                      <Text className={`ml-auto font-sans text-text-muted ${tv ? "text-tv-label" : "text-web-meta"}`}>
                        {runtimeLabel(episode.runtimeMinutes)}
                      </Text>
                    )}
                  </View>
                  {episode.plot && (
                    <Text
                      className={`font-sans text-text-secondary leading-body ${tv ? "text-tv-meta" : "text-web-meta"}`}
                      numberOfLines={tv ? 2 : 3}
                    >
                      {episode.plot}
                    </Text>
                  )}
                </View>
                {tv && focused && (
                  <View className="absolute inset-0 rounded-large border-(length:--spacing-focus-ring-tv-width) border-focus-ring" />
                )}
              </>
            )}
          </Pressable>
          {!tv && (
            <Pressable
              className="h-10 w-10 items-center justify-center rounded-full hover:bg-surface-raised"
              aria-label={`More about ${episode.title}`}
              onPress={() => router.push(`/episodes/${episode.id}`)}
            >
              <Text className="font-bold font-sans text-text-secondary text-web-body">⋯</Text>
            </Pressable>
          )}
        </View>
      ))}
    </View>
  );
}
