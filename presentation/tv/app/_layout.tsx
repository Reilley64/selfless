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
  const [signingIn, setSigningIn] = useState<string>();
  if (!serverUrl) {
    return <ServerAddress />;
  }
  if (users.length === 0) {
    return <SignIn onChangeServer={() => session.forgetServer()} />;
  }
  if (signingIn !== undefined) {
    return <SignIn name={signingIn} onDone={() => setSigningIn(undefined)} />;
  }
  if (!currentUserId) {
    return <UserPicker onSignIn={setSigningIn} />;
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
