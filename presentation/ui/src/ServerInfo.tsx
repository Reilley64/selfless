import { useGetHealth, useGetInfo } from "@selfless/api-client";
import { Text, View } from "react-native-css/components";

export function ServerInfo() {
  const { data } = useGetInfo();
  const { data: health } = useGetHealth();
  if (!data) {
    return null;
  }
  return (
    <View className="gap-1">
      <Text className="font-sans font-bold text-text-primary">{data.data.build.name}</Text>
      <Text className="font-sans text-text-secondary">{data.data.build.version}</Text>
      {health && <Text className="font-sans text-text-secondary">{health.data.status}</Text>}
    </View>
  );
}
