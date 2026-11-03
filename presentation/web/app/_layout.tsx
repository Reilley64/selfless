import { startTelemetry } from "@selfless/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Slot, usePathname } from "expo-router";
import { useSyncExternalStore } from "react";
import { View } from "react-native-css/components";
import { SignIn } from "../components/SignIn";
import { TopBar } from "../components/TopBar";
import { session } from "../session";

import "../global.css";

startTelemetry("selfless-web", session);

const queryClient = new QueryClient();
session.subscribe(() => queryClient.clear());

export default function Layout() {
  const token = useSyncExternalStore(session.subscribe, session.token);
  const pathname = usePathname();
  return (
    <QueryClientProvider client={queryClient}>
      {pathname === "/invite" ? (
        <Slot />
      ) : token ? (
        <View className="flex-1 bg-background">
          <TopBar />
          <Slot />
        </View>
      ) : (
        <SignIn />
      )}
    </QueryClientProvider>
  );
}
