import { ServerInfo } from "@selfless/ui";
import { View } from "react-native-css/components";

export default function Home() {
  return (
    <View className="flex-1 bg-background p-web-inset">
      <ServerInfo />
    </View>
  );
}
