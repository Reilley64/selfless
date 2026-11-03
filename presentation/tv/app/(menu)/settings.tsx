import { PlaybackSettingsToggles } from "@selfless/ui";
import { Text, View } from "react-native-css/components";

export default function Settings() {
  return (
    <View className="flex-1 gap-10 bg-background px-tv-inset pt-16">
      <Text role="heading" className="font-sans font-extrabold text-tv-title text-text-primary">
        Settings
      </Text>
      <PlaybackSettingsToggles autoFocus />
    </View>
  );
}
