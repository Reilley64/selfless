import { useGetEpisode } from "@selfless/api-client";
import { EpisodeHeader, PlayActions, playHref } from "@selfless/ui";
import { useLocalSearchParams } from "expo-router";
import { ScrollView, View } from "react-native-css/components";
import { ServerUnreachable } from "../../components/form";
import { NotFound } from "../../components/NotFound";

export default function Episode() {
  const { episodeId } = useLocalSearchParams<{ episodeId: string }>();
  const { data, isError } = useGetEpisode(episodeId);
  if (data?.status === 404) {
    return <NotFound />;
  }
  if (isError || (data && data.status !== 200)) {
    return (
      <View className="flex-1 bg-background px-web-inset pt-8">
        <ServerUnreachable />
      </View>
    );
  }
  if (!data) {
    return <View className="flex-1 bg-background" />;
  }
  const episode = data.data;
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="pb-16">
      <EpisodeHeader episode={episode}>
        <PlayActions href={playHref({ episodeId: episode.id })} resumePosition={episode.play.resumePosition} />
      </EpisodeHeader>
    </ScrollView>
  );
}
