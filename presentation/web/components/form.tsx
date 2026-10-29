import { Text } from "react-native-css/components";

export const input =
  "h-button-web-height rounded-small bg-surface-raised px-3 font-sans text-web-body text-text-primary";

export const primaryButton = (enabled: boolean) =>
  `h-button-web-height items-center justify-center rounded-small bg-button-focus ${enabled ? "" : "opacity-50"}`;

export function ServerUnreachable() {
  return <Text className="font-sans text-web-body text-accent">The server can&apos;t be reached. Try again.</Text>;
}
