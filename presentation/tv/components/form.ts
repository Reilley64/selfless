import type { FocusGuideInstance } from "@selfless/ui";
import { useEffect, useRef } from "react";

export const input =
  "h-button-tv-height rounded-pill bg-surface-raised font-sans text-tv-body text-text-primary placeholder:text-text-placeholder android:pl-6 android:pr-6 android:focus:bg-button-focus android:focus:text-text-inverse";

export const button =
  "h-button-tv-height items-center justify-center rounded-small bg-button-background focus:bg-button-focus disabled:opacity-50";

export const buttonText = (focused: boolean) =>
  `font-sans font-semibold text-tv-body ${focused ? "text-text-inverse" : "text-text-primary"}`;

export function useFirstFieldFocus() {
  const guide = useRef<FocusGuideInstance>(null);
  useEffect(() => guide.current?.requestTVFocus(), []);
  return guide;
}
