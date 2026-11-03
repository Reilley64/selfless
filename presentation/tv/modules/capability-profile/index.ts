import type { CapabilityProfile } from "@selfless/api-client";
import { requireNativeModule } from "expo";

export function androidTvProfile() {
  return requireNativeModule<{ capabilityProfile(): Promise<CapabilityProfile> }>("CapabilityProfile").capabilityProfile();
}
