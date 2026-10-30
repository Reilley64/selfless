import { useChangePassword, useGetMe } from "@selfless/api-client";
import { useRouter } from "expo-router";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { input, primaryButton, ServerUnreachable } from "../components/form";
import { session } from "../session";

export default function Settings() {
  const router = useRouter();
  const { data: me } = useGetMe();
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const changePassword = useChangePassword({
    mutation: {
      onSuccess: (response) => {
        if (response.status === 204) {
          setCurrentPassword("");
          setNewPassword("");
        }
      },
    },
  });
  const canChangePassword = currentPassword !== "" && newPassword !== "" && !changePassword.isPending;
  const submitPassword = () => canChangePassword && changePassword.mutate({ data: { currentPassword, newPassword } });
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
      <View className="w-full max-w-[360px] gap-4">
        <Text className="font-sans font-semibold text-web-body text-text-primary">Change password</Text>
        <TextInput
          className={input}
          placeholder="Current password"
          autoComplete="current-password"
          secureTextEntry
          value={currentPassword}
          onChangeText={setCurrentPassword}
          onSubmitEditing={submitPassword}
        />
        <TextInput
          className={input}
          placeholder="New password"
          autoComplete="new-password"
          secureTextEntry
          value={newPassword}
          onChangeText={setNewPassword}
          onSubmitEditing={submitPassword}
        />
        {changePassword.data?.status === 204 && (
          <Text className="font-sans text-web-body text-text-secondary">
            Password changed. You&apos;re signed out everywhere else.
          </Text>
        )}
        {changePassword.data?.status === 403 && (
          <Text className="font-sans text-web-body text-accent">The current password is wrong.</Text>
        )}
        {changePassword.isError && <ServerUnreachable />}
        <Pressable className={primaryButton(canChangePassword)} disabled={!canChangePassword} onPress={submitPassword}>
          <Text className="font-sans font-semibold text-web-body text-text-inverse">Change password</Text>
        </Pressable>
      </View>
      <Pressable
        className="h-button-web-height justify-center rounded-small bg-button-background px-4"
        onPress={signOut}
      >
        <Text className="font-sans font-semibold text-web-body text-text-primary">Sign out</Text>
      </Pressable>
    </View>
  );
}
