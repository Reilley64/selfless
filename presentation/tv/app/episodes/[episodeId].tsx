import { useGetEpisode } from "@selfless/api-client";
import { EpisodeHeader, FocusGuide, PlayActions, playHref } from "@selfless/ui";
import { useLocalSearchParams } from "expo-router";
import { ScrollView, View } from "react-native-css/components";
import { useFocusWhenLoaded } from "../../components/form";
import { NotFound } from "../../components/NotFound";
import { ServerUnreachable } from "../../components/ServerUnreachable";

export default function Episode() {
  const { episodeId } = useLocalSearchParams<{ episodeId: string }>();
  const { data, isError } = useGetEpisode(episodeId);
  const actions = useFocusWhenLoaded(data?.status === 200);
  if (data?.status === 404) {
    return <NotFound />;
  }
  if (isError || (data && data.status !== 200)) {
    return <ServerUnreachable />;
  }
  if (!data) {
    return <View className="flex-1 bg-background" />;
  }
  const episode = data.data;
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="pb-16">
      <EpisodeHeader episode={episode}>
        <FocusGuide autoFocus ref={actions} className="flex-row gap-3">
          <PlayActions href={playHref({ episodeId: episode.id })} resumePosition={episode.play.resumePosition} />
        </FocusGuide>
      </EpisodeHeader>
    </ScrollView>
  );
}
