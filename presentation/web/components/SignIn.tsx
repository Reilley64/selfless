import { useMutation } from "@tanstack/react-query";
import { getParser } from "bowser";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { session } from "../session";

function deviceName() {
  const userAgent = getParser(navigator.userAgent);
  return [userAgent.getBrowserName(), userAgent.getOSName()].filter(Boolean).join(" on ") || "Web browser";
}

export function SignIn() {
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const signIn = useMutation({ mutationFn: () => session.signIn(name, password, deviceName()) });
  const canSignIn = name !== "" && password !== "" && !signIn.isPending;
  const submit = () => canSignIn && signIn.mutate();
  const input = "h-button-web-height rounded-small bg-surface-raised px-3 font-sans text-web-body text-text-primary";

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
        {signIn.isError && (
          <Text className="font-sans text-web-body text-accent">The server can&apos;t be reached. Try again.</Text>
        )}
        <Pressable
          className={`h-button-web-height items-center justify-center rounded-small bg-button-focus ${canSignIn ? "" : "opacity-50"}`}
          disabled={!canSignIn}
          onPress={submit}
        >
          <Text className="font-sans font-semibold text-web-body text-text-inverse">Sign in</Text>
        </Pressable>
      </View>
    </View>
  );
}
