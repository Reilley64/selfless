import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { deviceName } from "../deviceName";
import { session } from "../session";
import { input, primaryButton, ServerUnreachable } from "./form";

export function SignIn() {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const signIn = useMutation({ mutationFn: () => session.signIn(name, password, deviceName()) });
  const canSignIn = name !== "" && password !== "" && !signIn.isPending;
  const submit = () => canSignIn && signIn.mutate();

  return (
    <View className="flex-1 items-center justify-center bg-background p-web-inset">
      <View className="w-full max-w-[360px] gap-4">
        <Text className="font-sans font-bold text-web-title text-text-primary">Sign in</Text>
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
        <TextInput
          className={input}
          placeholder="Password"
          autoComplete="current-password"
          secureTextEntry
          value={password}
          onChangeText={setPassword}
          onSubmitEditing={submit}
        />
        {signIn.data?.status === 401 && (
          <Text className="font-sans text-web-body text-accent">Wrong name or password.</Text>
        )}
        {signIn.isError && <ServerUnreachable />}
        <Pressable className={primaryButton(canSignIn)} disabled={!canSignIn} onPress={submit}>
          <Text className="font-sans font-semibold text-web-body text-text-inverse">Sign in</Text>
        </Pressable>
      </View>
    </View>
  );
}
