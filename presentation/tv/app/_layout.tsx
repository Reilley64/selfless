import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Slot } from "expo-router";
import { useState, useSyncExternalStore } from "react";
import { ServerAddress } from "../components/ServerAddress";
import { SignIn } from "../components/SignIn";
import { UserPicker } from "../components/UserPicker";
import { session } from "../session";

import "../global.css";

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
  return <Slot />;
}

export default function Layout() {
  return (
    <QueryClientProvider client={queryClient}>
      <Screen />
    </QueryClientProvider>
  );
}
