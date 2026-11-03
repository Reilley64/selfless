import { useGetShow } from "@selfless/api-client";
import {
  ActionButton,
  FocusGuide,
  playHref,
  ShowEpisodes,
  ShowHeader,
  showPlayLabel,
  TileRow,
  tilePath,
} from "@selfless/ui";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useRef } from "react";
import type { ScrollViewInstance } from "react-native";
import { ScrollView, View } from "react-native-css/components";
import { useFocusWhenLoaded } from "../../components/form";
import { NotFound } from "../../components/NotFound";
import { ServerUnreachable } from "../../components/ServerUnreachable";

export default function Show() {
  const { showId } = useLocalSearchParams<{ showId: string }>();
  const router = useRouter();
  const { data, isError } = useGetShow(showId);
  const scroll = useRef<ScrollViewInstance>(null);
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
  const show = data.data;
  return (
    <ScrollView ref={scroll} className="flex-1 bg-background" contentContainerClassName="gap-6 pb-16">
      <ShowHeader show={show}>
        <FocusGuide autoFocus ref={actions} className="flex-row gap-3">
          <ActionButton
            label={showPlayLabel(show.play)}
            primary
            onPress={() => router.push(playHref({ episodeId: show.play.episode.id }))}
            onFocus={() => scroll.current?.scrollTo({ y: 0, animated: true })}
          />
        </FocusGuide>
      </ShowHeader>
      <ShowEpisodes show={show} />
      {show.moreLikeThis.length > 0 && (
        <TileRow
          title="More like this"
          tiles={show.moreLikeThis}
          onPressTile={(tile) => router.push(tilePath(tile))}
          onFocusTile={() => scroll.current?.scrollToEnd({ animated: true })}
        />
      )}
    </ScrollView>
  );
}
