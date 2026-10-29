import { startTelemetry } from "@selfless/ui";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Slot } from "expo-router";

import "../global.css";

startTelemetry("selfless-tv", process.env.EXPO_PUBLIC_SERVER_URL);

const queryClient = new QueryClient();

export default function Layout() {
  return (
    <QueryClientProvider client={queryClient}>
      <Slot />
    </QueryClientProvider>
  );
}
