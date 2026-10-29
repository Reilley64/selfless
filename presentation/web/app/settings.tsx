import { useGetMe } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { Pressable, Text, View } from "react-native-css/components";
import { session } from "../session";

export default function Settings() {
  const router = useRouter();
  const { data: me } = useGetMe();
  const signOut = () => {
    router.replace("/");
    void session.signOut();
  };

  return (
    <View className="flex-1 items-start gap-6 bg-background p-web-inset">
      <Text className="font-sans font-bold text-web-title text-text-primary">Settings</Text>
      {me?.status === 200 && (
        <Text className="font-sans text-web-body text-text-secondary">Signed in as {me.data.name}</Text>
      )}
      <Pressable
        className="h-button-web-height justify-center rounded-small bg-button-background px-4"
        onPress={signOut}
      >
        <Text className="font-sans font-semibold text-web-body text-text-primary">Sign out</Text>
      </Pressable>
    </View>
  );
}
