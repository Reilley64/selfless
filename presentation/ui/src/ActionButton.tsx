import type { Ref } from "react";
import { type View as HostView, Platform } from "react-native";
import { Pressable, Text } from "react-native-css/components";

export function ActionButton({
  ref,
  label,
  primary = false,
  small = false,
  onPress,
  onFocus,
}: {
  ref?: Ref<HostView>;
  label: string;
  primary?: boolean;
  small?: boolean;
  onPress: () => void;
  onFocus?: () => void;
}) {
  const size = Platform.isTV
    ? "h-button-tv-height px-6"
    : small
      ? "h-[34px] px-4"
      : "h-button-web-height px-5";
  const text = Platform.isTV ? "text-tv-body" : small ? "text-web-meta" : "text-web-body";
  return (
    <Pressable
      ref={ref}
      role="button"
      className={`group justify-center rounded-small transition-transform hover:scale-105 hover:bg-button-focus focus:scale-105 focus:bg-button-focus ${size} ${primary ? "bg-button-primary" : "bg-button-background"}`}
      onPress={onPress}
      onFocus={onFocus}
    >
      <Text
        className={`font-sans font-bold text-text-primary group-hover:text-text-inverse group-focus:text-text-inverse ${text}`}
      >
        {label}
      </Text>
    </Pressable>
  );
}
