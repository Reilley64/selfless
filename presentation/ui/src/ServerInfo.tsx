import { useGetHealth, useGetInfo } from "@selfless/api-client";
import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";

export function ServerInfo() {
  const { data } = useGetInfo();
  const { data: health } = useGetHealth();
  if (!data) {
    return null;
  }
  return (
    <View className="gap-1" focusable={false}>
      <Text className={`font-sans font-bold text-text-primary ${Platform.isTV ? "text-tv-title" : "text-web-title"}`}>
        {data.data.build.name}
      </Text>
      <Text className={`font-sans text-text-secondary ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}>
        {data.data.build.version}
      </Text>
      {health && (
        <Text className={`font-sans text-text-secondary ${Platform.isTV ? "text-tv-body" : "text-web-body"}`}>
          {health.data.status}
        </Text>
      )}
    </View>
  );
}
