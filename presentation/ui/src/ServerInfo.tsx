import { useGetInfo } from "@selfless/api-client";
import { Text, View } from "react-native";

export function ServerInfo() {
  const { data } = useGetInfo();
  if (!data) {
    return null;
  }
  return (
    <View>
      <Text>{data.data.build.name}</Text>
      <Text>{data.data.build.version}</Text>
    </View>
  );
}
