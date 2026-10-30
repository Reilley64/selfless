import { acceptInvite } from "@selfless/api-client";
import { useMutation } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { input, primaryButton, ServerUnreachable } from "../components/form";
import { deviceName } from "../deviceName";
import { session } from "../session";

function randomUuid() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (x) => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export default function Invite() {
  const router = useRouter();
  const [token] = useState(() => window.location.hash.slice(1));
  const [idempotencyKey] = useState(randomUuid);
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const accept = useMutation({
    mutationFn: async () => {
      const accepted = await acceptInvite({ token, name, password }, { "Idempotency-Key": idempotencyKey });
      if (accepted.status === 201) {
        await session.signIn(name, password, deviceName());
        router.replace("/");
      }
      return accepted;
    },
  });
  const canAccept = name !== "" && password !== "" && !accept.isPending;
  const submit = () => canAccept && accept.mutate();

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
        {accept.isError && <ServerUnreachable />}
        <Pressable className={primaryButton(canAccept)} disabled={!canAccept} onPress={submit}>
          <Text className="font-sans font-semibold text-web-body text-text-inverse">Join</Text>
        </Pressable>
      </View>
    </View>
  );
}
