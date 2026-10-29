import { ClientKind } from "@selfless/api-client";
import { startTelemetry, startTvSession } from "@selfless/ui";
import { deleteItemAsync, getItem, setItem } from "expo-secure-store";
import { Platform } from "react-native";

export const session = startTvSession(
  { getItem, setItem, removeItem: (key) => void deleteItemAsync(key) },
  Platform.OS === "ios" ? ClientKind.appleTv : ClientKind.googleTv,
);

startTelemetry("selfless-tv", session);
