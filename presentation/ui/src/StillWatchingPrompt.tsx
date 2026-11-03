import { useEffect, useRef } from "react";
import { Platform, type View as HostView } from "react-native";
import { Text, View } from "react-native-css/components";
import { ActionButton } from "./ActionButton";
import type { FocusGuideInstance } from "./FocusGuide";

const focusDelay = 100;

export function StillWatchingPrompt({
  showTitle,
  onKeepWatching,
  onBack,
}: {
  showTitle: string;
  onKeepWatching: () => void;
  onBack: () => void;
}) {
  const keepWatching = useRef<HostView>(null);
  useEffect(() => {
    if (!Platform.isTV) {
      return;
    }
    const timeout = setTimeout(
      () => (keepWatching.current as unknown as FocusGuideInstance | null)?.requestTVFocus(),
      focusDelay,
    );
    return () => clearTimeout(timeout);
  }, []);
  return (
    <View
      className={`absolute inset-0 items-start justify-center bg-scrim ${Platform.isTV ? "gap-8 px-tv-inset" : "gap-6 px-web-inset"}`}
    >
      <Text
        role="heading"
        className={`font-extrabold font-sans text-text-primary ${Platform.isTV ? "text-tv-title" : "text-web-title"}`}
      >
        {`Are you still watching ${showTitle}?`}
      </Text>
      <View className="flex-row gap-3">
        <ActionButton ref={keepWatching} label="Keep watching" primary onPress={onKeepWatching} />
        <ActionButton label="Back" onPress={onBack} />
      </View>
    </View>
  );
}
