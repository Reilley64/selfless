import { useChangePlaybackSettings, useGetPlaybackSettings } from "@selfless/api-client";
import { useRef } from "react";
import { Platform, type View as HostView } from "react-native";
import { Pressable, Text, View } from "react-native-css/components";
import type { FocusGuideInstance } from "./FocusGuide";
import { serverUnreachable } from "./messages";

const toggles = [
  { setting: "autoplayNextEpisode", label: "Autoplay next episode" },
  { setting: "askIfStillWatching", label: "Ask if I'm still watching" },
] as const;

export function PlaybackSettingsToggles({ autoFocus = false }: { autoFocus?: boolean }) {
  const stored = useGetPlaybackSettings();
  const change = useChangePlaybackSettings();
  const focusOnLoad = useRef(autoFocus);
  const settings = change.isPending
    ? change.variables.data
    : change.data?.status === 200
      ? change.data.data
      : stored.data?.status === 200
        ? stored.data.data
        : undefined;
  const text = Platform.isTV ? "text-tv-body" : "text-web-body";
  const focusFirstToggle = (toggle: HostView | null) => {
    if (toggle && focusOnLoad.current) {
      focusOnLoad.current = false;
      (toggle as unknown as FocusGuideInstance).requestTVFocus();
    }
  };

  return (
    <View className={Platform.isTV ? "w-[760px] gap-3.5" : "w-full max-w-[360px] gap-2"}>
      {settings &&
        toggles.map(({ setting, label }, index) => (
          <Pressable
            key={setting}
            ref={index === 0 ? focusFirstToggle : undefined}
            role="switch"
            aria-checked={settings[setting]}
            aria-label={label}
            className={`group flex-row items-center justify-between rounded-small bg-button-background hover:bg-button-focus focus:bg-button-focus ${Platform.isTV ? "h-button-tv-height px-6 transition-transform focus:scale-105" : "h-button-web-height px-4"}`}
            onPress={() => change.mutate({ data: { ...settings, [setting]: !settings[setting] } })}
          >
            <Text
              className={`font-sans font-semibold text-text-primary group-hover:text-text-inverse group-focus:text-text-inverse ${text}`}
            >
              {label}
            </Text>
            <View
              className={`justify-center rounded-pill ${Platform.isTV ? "h-10 w-[72px] px-1" : "h-6 w-11 px-0.5"} ${settings[setting] ? "items-end bg-accent" : "items-start bg-surface-raised group-hover:bg-text-inverse group-focus:bg-text-inverse"}`}
            >
              <View className={`rounded-pill bg-text-primary ${Platform.isTV ? "size-8" : "size-5"}`} />
            </View>
          </Pressable>
        ))}
      {(stored.isError || change.isError) && (
        <Text className={`font-sans text-accent ${text}`}>{serverUnreachable}</Text>
      )}
    </View>
  );
}
