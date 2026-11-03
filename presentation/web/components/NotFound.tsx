import { Link } from "expo-router";
import { Pressable, Text, View } from "react-native-css/components";

export function NotFound({ message = "Nothing here" }: { message?: string }) {
  return (
    <View className="flex-1 gap-4 bg-background px-web-inset pt-8">
      <Text className="font-sans font-extrabold text-web-heading text-text-primary">{message}</Text>
      <Link href="/" asChild>
        <Pressable>
          <Text className="font-sans text-web-body text-accent">Back to home</Text>
        </Pressable>
      </Link>
    </View>
  );
}
