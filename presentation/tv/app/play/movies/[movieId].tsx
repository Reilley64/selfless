import { useGetMovie } from "@selfless/api-client";
import { useLocalSearchParams } from "expo-router";
import { View } from "react-native-css/components";
import { NotFound } from "../../../components/NotFound";
import { Player } from "../../../components/Player";
import { ServerUnreachable } from "../../../components/ServerUnreachable";

export default function PlayMovie() {
  const { movieId, position } = useLocalSearchParams<{ movieId: string; position?: string }>();
  const { data, isError } = useGetMovie(movieId, { query: { staleTime: Infinity, refetchOnWindowFocus: false } });
  if (data?.status === 404 || (data?.status === 200 && !data.data.mediaFileId)) {
    return <NotFound />;
  }
  if (isError || (data && data.status !== 200)) {
    return <ServerUnreachable />;
  }
  if (!data?.data.mediaFileId) {
    return <View className="flex-1 bg-black" />;
  }
  return (
    <Player
      mediaFileId={data.data.mediaFileId}
      startPosition={position === undefined ? (data.data.play.resumePosition ?? 0) : Number(position)}
    />
  );
}
