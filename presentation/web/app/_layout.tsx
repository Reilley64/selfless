import { startTelemetry } from "@selfless/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Slot } from "expo-router";
import { useSyncExternalStore } from "react";
import { SignIn } from "../components/SignIn";
import { session } from "../session";

import "../global.css";

startTelemetry("selfless-web");

const queryClient = new QueryClient();
session.subscribe(() => queryClient.clear());

export default function Layout() {
  const token = useSyncExternalStore(session.subscribe, session.token);
  return <QueryClientProvider client={queryClient}>{token ? <Slot /> : <SignIn />}</QueryClientProvider>;
}
