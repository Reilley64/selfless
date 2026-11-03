import { Text, View } from "react-native-css/components";

export function Placeholder({ title }: { title: string }) {
  return (
    <View className="flex-1 bg-background px-tv-inset pt-16">
      <Text role="heading" className="font-sans font-extrabold text-tv-title text-text-primary">
        {title}
      </Text>
    </View>
  );
}
