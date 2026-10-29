import { acceptInvite } from "@selfless/api-client";
import { useMutation } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { deviceName } from "../deviceName";
import { session } from "../session";

export default function Invite() {
  const router = useRouter();
  const [token] = useState(() => window.location.hash.slice(1));
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const accept = useMutation({
    mutationFn: async () => {
      const accepted = await acceptInvite({ token, name, password });
      if (accepted.status === 201) {
        await session.signIn(name, password, deviceName());
        router.replace("/");
      }
      return accepted;
    },
  });
  const canAccept = name !== "" && password !== "" && !accept.isPending;
  const submit = () => canAccept && accept.mutate();
  const input = "h-button-web-height rounded-small bg-surface-raised px-3 font-sans text-web-body text-text-primary";

  if (token === "" || accept.data?.status === 404) {
    return (
      <View className="flex-1 items-center justify-center bg-background p-web-inset">
        <View className="w-full max-w-[360px] gap-4">
          <Text className="font-sans font-bold text-web-title text-text-primary">This link no longer works</Text>
          <Text className="font-sans text-web-body text-text-secondary">Ask an admin for a new invite.</Text>
        </View>
      </View>
    );
  }

  return (
    <View className="flex-1 items-center justify-center bg-background p-web-inset">
      <View className="w-full max-w-[360px] gap-4">
        <Text className="font-sans font-bold text-web-title text-text-primary">Join selfless</Text>
        <Text className="font-sans text-web-body text-text-secondary">
          Choose the name and password you will sign in with.
        </Text>
        <TextInput
          className={input}
          placeholder="Name"
          autoComplete="username"
          autoCapitalize="none"
          autoFocus
          value={name}
          onChangeText={setName}
          onSubmitEditing={submit}
        />
        {accept.data?.status === 409 && (
          <Text className="font-sans text-web-body text-accent">This name is taken. Choose another.</Text>
        )}
        <TextInput
          className={input}
          placeholder="Password"
          autoComplete="new-password"
          secureTextEntry
          value={password}
          onChangeText={setPassword}
          onSubmitEditing={submit}
        />
        {accept.isError && (
          <Text className="font-sans text-web-body text-accent">The server can&apos;t be reached. Try again.</Text>
        )}
        <Pressable
          className={`h-button-web-height items-center justify-center rounded-small bg-button-focus ${canAccept ? "" : "opacity-50"}`}
          disabled={!canAccept}
          onPress={submit}
        >
          <Text className="font-sans font-semibold text-web-body text-text-inverse">Join</Text>
        </Pressable>
      </View>
    </View>
  );
}
