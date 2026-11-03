import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Stack } from "expo-router";
import { useState, useSyncExternalStore } from "react";
import { Appearance, useWindowDimensions, View } from "react-native";
import { ServerAddress } from "../components/ServerAddress";
import { SignIn } from "../components/SignIn";
import { UserPicker } from "../components/UserPicker";
import { session } from "../session";

import "../global.css";

const DESIGN_WIDTH = 1920;

Appearance.setColorScheme("dark");
const queryClient = new QueryClient();
session.subscribe(() => queryClient.clear());

function Screen() {
  const { serverUrl, users, currentUserId } = useSyncExternalStore(session.subscribe, session.state);
  const [signInScreen, setSignInScreen] = useState<{ open: false } | { open: true; name: string }>({ open: false });
  if (!serverUrl) {
    return <ServerAddress />;
  }
  if (users.length === 0) {
    return <SignIn onChangeServer={() => session.forgetServer()} />;
  }
  if (signInScreen.open) {
    return <SignIn name={signInScreen.name} onDone={() => setSignInScreen({ open: false })} />;
  }
  if (!currentUserId) {
    return <UserPicker onSignIn={(name) => setSignInScreen({ open: true, name })} />;
  }
  return <Stack screenOptions={{ headerShown: false }} />;
}

export default function Layout() {
  const { width, height } = useWindowDimensions();
  const scale = width / DESIGN_WIDTH;
  return (
    <QueryClientProvider client={queryClient}>
      <View style={{ width: DESIGN_WIDTH, height: height / scale, transform: [{ scale }], transformOrigin: "top left" }}>
        <Screen />
      </View>
    </QueryClientProvider>
  );
}
