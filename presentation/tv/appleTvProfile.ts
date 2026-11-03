import type { CapabilityProfile } from "@selfless/api-client";

export const appleTvProfile: CapabilityProfile = {
  containers: ["mp4", "mov"],
  videoCodecs: [
    {
      codec: "h264",
      profiles: ["constrainedbaseline", "baseline", "main", "high"],
      maxLevel: 52,
      maxWidth: 3840,
      maxHeight: 2160,
      bitDepths: [8],
      hdrTypes: ["sdr"],
      dolbyVisionProfiles: [],
    },
    {
      codec: "hevc",
      profiles: ["main", "main10"],
      maxLevel: 153,
      maxWidth: 3840,
      maxHeight: 2160,
      bitDepths: [8, 10],
      hdrTypes: ["sdr", "hdr10", "hlg", "dolbyVision"],
      dolbyVisionProfiles: [8],
    },
  ],
  audioCodecs: [
    { codec: "aac", maxChannels: 8 },
    { codec: "ac3", maxChannels: 6 },
    { codec: "eac3", maxChannels: 8 },
    { codec: "flac", maxChannels: 8 },
    { codec: "alac", maxChannels: 8 },
    { codec: "mp3", maxChannels: 2 },
  ],
  subtitleCodecs: [],
};
