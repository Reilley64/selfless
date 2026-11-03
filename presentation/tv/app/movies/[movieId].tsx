import { useGetMovie } from "@selfless/api-client";
import { FocusGuide, MovieHeader, PlayActions, playHref, TileRow, tilePath } from "@selfless/ui";
import { useLocalSearchParams, useRouter } from "expo-router";
import { useRef } from "react";
import type { ScrollViewInstance } from "react-native";
import { ScrollView, View } from "react-native-css/components";
import { useFocusWhenLoaded } from "../../components/form";
import { NotFound } from "../../components/NotFound";
import { ServerUnreachable } from "../../components/ServerUnreachable";

export default function Movie() {
  const { movieId } = useLocalSearchParams<{ movieId: string }>();
  const router = useRouter();
  const { data, isError } = useGetMovie(movieId);
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
  const movie = data.data;
  return (
    <ScrollView ref={scroll} className="flex-1 bg-background" contentContainerClassName="pb-16">
      <MovieHeader movie={movie}>
        <FocusGuide autoFocus ref={actions} className="flex-row gap-3">
          <PlayActions
            href={playHref({ movieId: movie.id })}
            resumePosition={movie.play.resumePosition}
            onFocus={() => scroll.current?.scrollTo({ y: 0, animated: true })}
          />
        </FocusGuide>
      </MovieHeader>
      {movie.moreLikeThis.length > 0 && (
        <TileRow
          title="More like this"
          tiles={movie.moreLikeThis}
          onPressTile={(tile) => router.push(tilePath(tile))}
          onFocusTile={() => scroll.current?.scrollToEnd({ animated: true })}
        />
      )}
    </ScrollView>
  );
}
