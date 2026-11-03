import { Platform } from "react-native";
import { Text, View } from "react-native-css/components";

export function WatchedMark({ className = "" }: { className?: string }) {
  return (
    <View
      aria-label="Watched"
      className={`items-center justify-center rounded-full bg-scrim ${Platform.isTV ? "h-9 w-9" : "h-6 w-6"} ${className}`}
    >
      <Text className={`font-bold font-sans text-text-primary ${Platform.isTV ? "text-tv-meta" : "text-web-label"}`}>
        ✓
      </Text>
    </View>
  );
}
