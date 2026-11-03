import { useListSeasonEpisodes } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { Pressable, Text, View } from "react-native-css/components";
import { playHref } from "./playHref";
import { runtimeLabel } from "./runtimeLabel";
import { ArtworkImage } from "./Tile";

export function EpisodeList({ seasonId }: { seasonId: string }) {
  const router = useRouter();
  const { data } = useListSeasonEpisodes(seasonId);
  const episodes = data?.status === 200 ? data.data.episodes : [];
  return (
    <View className="max-w-[1100px] px-web-inset">
      {episodes.map((episode) => (
        <View key={episode.id} className="flex-row items-center gap-2 border-surface-raised border-b">
          <Pressable
            className="group flex-1 flex-row items-center gap-5 rounded-large p-4 hover:bg-surface-raised"
            aria-label={`Play E${episode.number} ${episode.title}`}
            onPress={() => router.push(playHref({ episodeId: episode.id }))}
          >
            <Text className="w-10 text-center font-black font-sans text-text-muted text-web-heading">
              {episode.number}
            </Text>
            <View className="h-[117px] w-[208px] overflow-hidden rounded-small bg-surface">
              {episode.thumb && (
                <ArtworkImage
                  artwork={episode.thumb}
                  testID="episode-thumb"
                  className="h-full w-full"
                  resizeMode="cover"
                />
              )}
            </View>
            <View className="flex-1 gap-1">
              <View className="flex-row items-center gap-3">
                <Text className="shrink font-bold font-sans text-text-primary text-web-body">{episode.title}</Text>
                {episode.runtimeMinutes != null && episode.runtimeMinutes > 0 && (
                  <Text className="ml-auto font-sans text-text-muted text-web-meta">
                    {runtimeLabel(episode.runtimeMinutes)}
                  </Text>
                )}
              </View>
              {episode.plot && (
                <Text className="font-sans text-text-secondary text-web-meta leading-body" numberOfLines={3}>
                  {episode.plot}
                </Text>
              )}
            </View>
          </Pressable>
          <Pressable
            className="h-10 w-10 items-center justify-center rounded-full hover:bg-surface-raised"
            aria-label={`More about ${episode.title}`}
            onPress={() => router.push(`/episodes/${episode.id}`)}
          >
            <Text className="font-bold font-sans text-text-secondary text-web-body">⋯</Text>
          </Pressable>
        </View>
      ))}
    </View>
  );
}
