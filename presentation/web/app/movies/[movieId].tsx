import { useGetMovie } from "@selfless/api-client";
import { MovieHeader, TileRow, tilePath } from "@selfless/ui";
import { useLocalSearchParams, useRouter } from "expo-router";
import { ScrollView, View } from "react-native-css/components";
import { ServerUnreachable } from "../../components/form";
import { NotFound } from "../../components/NotFound";
import { PlayButtons } from "../../components/PlayButtons";

export default function Movie() {
  const { movieId } = useLocalSearchParams<{ movieId: string }>();
  const router = useRouter();
  const { data, isError } = useGetMovie(movieId);
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
  const movie = data.data;
  return (
    <ScrollView className="flex-1 bg-background" contentContainerClassName="gap-6 pb-16">
      <MovieHeader movie={movie}>
        <PlayButtons href={`/play/movies/${movie.id}`} resumePosition={movie.play.resumePosition} />
      </MovieHeader>
      {movie.moreLikeThis.length > 0 && (
        <TileRow
          title="More like this"
          tiles={movie.moreLikeThis}
          onPressTile={(tile) => router.push(tilePath(tile))}
        />
      )}
    </ScrollView>
  );
}
