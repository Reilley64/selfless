import { serverUnreachable } from "@selfless/ui";
import { Text, View } from "react-native-css/components";

export function ServerUnreachable() {
  return (
    <View className="flex-1 justify-center bg-background px-tv-inset">
      <Text className="font-sans text-tv-body text-text-muted">{serverUnreachable}</Text>
    </View>
  );
}
