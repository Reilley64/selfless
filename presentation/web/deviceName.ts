import { getParser } from "bowser";

export function deviceName() {
  const userAgent = getParser(navigator.userAgent);
  return [userAgent.getBrowserName(), userAgent.getOSName()].filter(Boolean).join(" on ") || "Web browser";
}
