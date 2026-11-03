import { useGetShow } from "@selfless/api-client";
import { playHref, ShowEpisodes, ShowHeader, showPlayLabel, TileRow, tilePath } from "@selfless/ui";
import { Link, useLocalSearchParams, useRouter } from "expo-router";
import { Pressable, ScrollView, Text, View } from "react-native-css/components";
import { ServerUnreachable } from "../../components/form";
import { NotFound } from "../../components/NotFound";

export default function Show() {
  const { showId } = useLocalSearchParams<{ showId: string }>();
  const router = useRouter();
  const { data, isError } = useGetShow(showId);
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
  const show = data.data;
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="gap-6 pb-16">
      <ShowHeader show={show}>
        <Link href={playHref({ episodeId: show.play.episode.id })} asChild>
          <Pressable className="h-button-web-height justify-center rounded-small bg-button-primary px-5 hover:bg-button-focus">
            <Text className="font-sans font-bold text-web-body text-text-primary">{showPlayLabel(show.play)}</Text>
          </Pressable>
        </Link>
      </ShowHeader>
      <ShowEpisodes show={show} />
      {show.moreLikeThis.length > 0 && (
        <TileRow
          title="More like this"
          tiles={show.moreLikeThis}
          onPressTile={(tile) => router.push(tilePath(tile))}
        />
      )}
    </ScrollView>
  );
}
