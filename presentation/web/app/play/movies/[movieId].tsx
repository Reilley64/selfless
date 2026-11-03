import { useGetMovie } from "@selfless/api-client";
import { useLocalSearchParams } from "expo-router";
import { View } from "react-native-css/components";
import { ServerUnreachable } from "../../../components/form";
import { NotFound } from "../../../components/NotFound";
import { Player } from "../../../components/Player";

export default function PlayMovie() {
  const { movieId, position } = useLocalSearchParams<{ movieId: string; position?: string }>();
  const { data, isError } = useGetMovie(movieId, { query: { staleTime: Infinity, refetchOnWindowFocus: false } });
  if (data?.status === 404 || (data?.status === 200 && !data.data.mediaFileId)) {
    return <NotFound />;
  }
  if (isError || (data && data.status !== 200)) {
    return (
      <View className="flex-1 bg-background px-web-inset pt-8">
        <ServerUnreachable />
      </View>
    );
  }
  if (!data?.data.mediaFileId) {
    return <View className="flex-1 bg-black" />;
  }
  return (
    <Player
      mediaFileId={data.data.mediaFileId}
      startPosition={position === undefined ? (data.data.play.resumePosition ?? 0) : Number(position)}
      backHref={`/movies/${movieId}`}
      titleHref={`/movies/${movieId}`}
    />
  );
}
