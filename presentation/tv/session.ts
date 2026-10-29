import { startTelemetry, startTvSession } from "@selfless/ui";
import { deleteItemAsync, getItem, setItem } from "expo-secure-store";
import { Platform } from "react-native";

export const session = startTvSession(
  { getItem, setItem, removeItem: (key) => void deleteItemAsync(key) },
  Platform.OS === "ios" ? "appleTv" : "googleTv",
);

let telemetryStarted = false;

function startTelemetryOnceConnected() {
  const { serverUrl } = session.state();
  if (serverUrl && !telemetryStarted) {
    telemetryStarted = true;
    startTelemetry("selfless-tv", session);
  }
}

startTelemetryOnceConnected();
session.subscribe(startTelemetryOnceConnected);
