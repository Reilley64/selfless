export const input =
  "h-button-tv-height rounded-small bg-surface-raised px-6 font-sans text-tv-body text-text-primary focus:bg-button-primary";

export const button =
  "h-button-tv-height items-center justify-center rounded-small bg-button-background focus:bg-button-focus";

export const buttonText = (focused: boolean) =>
  `font-sans font-semibold text-tv-body ${focused ? "text-text-inverse" : "text-text-primary"}`;
