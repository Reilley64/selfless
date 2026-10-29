import { FocusGuide } from "@selfless/ui";
import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native-css/components";
import { session } from "../session";

export function ServerAddress() {
  const [address, setAddress] = useState("");
  const connect = useMutation({ mutationFn: () => session.connect(address) });
  const canConnect = address.trim() !== "" && !connect.isPending;
  const submit = () => canConnect && connect.mutate();

  return (
    <FocusGuide autoFocus className="flex-1 justify-center bg-background p-tv-inset">
      <View className="w-[760px] gap-6">
        <Text className="font-sans font-bold text-tv-title text-text-primary">Server address</Text>
        <Text className="font-sans text-tv-body text-text-secondary">
          Enter the address of your selfless server, such as 192.168.1.20:8080.
        </Text>
        <TextInput
          className="h-button-tv-height rounded-small bg-surface-raised px-6 font-sans text-tv-body text-text-primary focus:bg-button-primary"
          placeholder="Server address"
          keyboardType="url"
          textContentType="URL"
          autoCapitalize="none"
          autoCorrect={false}
          value={address}
          onChangeText={setAddress}
          onSubmitEditing={submit}
        />
        {connect.data === false && (
          <Text className="font-sans text-tv-body text-accent">No selfless server answers at that address.</Text>
        )}
        <Pressable
          className="h-button-tv-height items-center justify-center rounded-small bg-button-background focus:bg-button-focus"
          disabled={!canConnect}
          onPress={submit}
        >
          {({ focused }) => (
            <Text
              className={`font-sans font-semibold text-tv-body ${focused ? "text-text-inverse" : "text-text-primary"}`}
            >
              Connect
            </Text>
          )}
        </Pressable>
      </View>
    </FocusGuide>
  );
}
