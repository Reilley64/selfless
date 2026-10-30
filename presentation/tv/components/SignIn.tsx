import { FocusGuide, serverUnreachable } from "@selfless/ui";
import { useMutation } from "@tanstack/react-query";
import { deviceName } from "expo-device";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { session } from "../session";
import { button, buttonText, input } from "./form";

export function SignIn({
  name: filledIn = "",
  onDone,
  onChangeServer,
}: {
  name?: string;
  onDone?: () => void;
  onChangeServer?: () => void;
}) {
  const [name, setName] = useState(filledIn);
  const [password, setPassword] = useState("");
  const signIn = useMutation({
    mutationFn: () => session.signIn(name, password, deviceName ?? "TV"),
    onSuccess: (response) => response.status === 201 && onDone?.(),
  });
  const canSignIn = name !== "" && password !== "" && !signIn.isPending;
  const submit = () => canSignIn && signIn.mutate();

  return (
    <FocusGuide autoFocus className="flex-1 justify-center bg-background p-tv-inset">
      <View className="w-[760px] gap-6">
        <Text className="font-sans font-bold text-tv-title text-text-primary">Sign in</Text>
        <TextInput
          className={input}
          placeholder="Name"
          textContentType="username"
          autoComplete="username"
          autoCapitalize="none"
          autoCorrect={false}
          value={name}
          onChangeText={setName}
        />
        <TextInput
          className={input}
          placeholder="Password"
          textContentType="password"
          autoComplete="password"
          secureTextEntry
          value={password}
          onChangeText={setPassword}
          onSubmitEditing={submit}
        />
        {signIn.data?.status === 401 && (
          <Text className="font-sans text-tv-body text-accent">Wrong name or password.</Text>
        )}
        {signIn.isError && (
          <Text className="font-sans text-tv-body text-accent">{serverUnreachable}</Text>
        )}
        <Pressable className={button} disabled={!canSignIn} onPress={submit}>
          {({ focused }) => <Text className={buttonText(focused)}>Sign in</Text>}
        </Pressable>
        {onDone && (
          <Pressable className={button} onPress={onDone}>
            {({ focused }) => <Text className={buttonText(focused)}>Cancel</Text>}
          </Pressable>
        )}
        {onChangeServer && (
          <Pressable className={button} onPress={onChangeServer}>
            {({ focused }) => <Text className={buttonText(focused)}>Change server</Text>}
          </Pressable>
        )}
      </View>
    </FocusGuide>
  );
}
