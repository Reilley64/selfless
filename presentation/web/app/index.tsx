import { ServerInfo } from "@selfless/ui";
import { Link } from "expo-router";
import { Pressable, Text, View } from "react-native-css/components";

export default function Home() {
  return (
    <View className="flex-1 gap-6 bg-background p-web-inset">
      <ServerInfo />
      <Link href="/settings" asChild>
        <Pressable>
          <Text className="font-sans text-web-body text-text-secondary">Settings</Text>
        </Pressable>
      </Link>
    </View>
  );
}
