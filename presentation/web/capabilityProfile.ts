import type { AudioCodecSupport, CapabilityProfile, HdrType, VideoCodecSupport } from "@selfless/api-client";

type VideoCandidate = { codec: string; profiles: string[]; contentType: string; tenBit: boolean };

const videoCandidates: VideoCandidate[] = [
  { codec: "h264", profiles: ["constrainedbaseline", "baseline"], contentType: 'video/mp4; codecs="avc1.42E01E"', tenBit: false },
  { codec: "h264", profiles: ["main"], contentType: 'video/mp4; codecs="avc1.4D4028"', tenBit: false },
  { codec: "h264", profiles: ["high"], contentType: 'video/mp4; codecs="avc1.640033"', tenBit: false },
  { codec: "hevc", profiles: ["main"], contentType: 'video/mp4; codecs="hvc1.1.6.L153.B0"', tenBit: false },
  { codec: "hevc", profiles: ["main10"], contentType: 'video/mp4; codecs="hvc1.2.4.L153.B0"', tenBit: true },
  { codec: "vp9", profiles: ["profile0"], contentType: 'video/webm; codecs="vp09.00.51.08"', tenBit: false },
  { codec: "vp9", profiles: ["profile2"], contentType: 'video/webm; codecs="vp09.02.51.10"', tenBit: true },
  { codec: "av1", profiles: ["main"], contentType: 'video/mp4; codecs="av01.0.13M.10"', tenBit: true },
];

const sizes = [
  { width: 3840, height: 2160, bitrate: 40_000_000 },
  { width: 1920, height: 1080, bitrate: 10_000_000 },
];

const hdrCandidates: { hdrType: HdrType; transferFunction: TransferFunction; hdrMetadataType?: HdrMetadataType }[] = [
  { hdrType: "hdr10", transferFunction: "pq", hdrMetadataType: "smpteSt2086" },
  { hdrType: "hlg", transferFunction: "hlg" },
];

const audioCandidates = [
  { codec: "aac", contentType: 'audio/mp4; codecs="mp4a.40.2"' },
  { codec: "mp3", contentType: 'audio/mp4; codecs="mp4a.6B"' },
  { codec: "flac", contentType: 'audio/mp4; codecs="flac"' },
  { codec: "opus", contentType: 'audio/webm; codecs="opus"' },
  { codec: "vorbis", contentType: 'audio/webm; codecs="vorbis"' },
  { codec: "ac3", contentType: 'audio/mp4; codecs="ac-3"' },
  { codec: "eac3", contentType: 'audio/mp4; codecs="ec-3"' },
];

function supported(configuration: MediaDecodingConfiguration) {
  return navigator.mediaCapabilities.decodingInfo(configuration).then(
    (x) => x.supported,
    () => false,
  );
}

function video(contentType: string, size: (typeof sizes)[number], hdr?: (typeof hdrCandidates)[number]) {
  return supported({
    type: "file",
    video: {
      contentType,
      width: size.width,
      height: size.height,
      bitrate: size.bitrate,
      framerate: 24,
      ...(hdr && { colorGamut: "rec2020", transferFunction: hdr.transferFunction, hdrMetadataType: hdr.hdrMetadataType }),
    },
  });
}

async function videoSupport(candidate: VideoCandidate, highDynamicRange: boolean) {
  const fits = await Promise.all(sizes.map((size) => video(candidate.contentType, size)));
  const size = sizes.find((_, index) => fits[index]);
  if (!size) {
    return undefined;
  }
  const hdr = highDynamicRange && candidate.tenBit ? hdrCandidates : [];
  const hdrFits = await Promise.all(hdr.map((x) => video(candidate.contentType, size, x)));
  return {
    codec: candidate.codec,
    profiles: candidate.profiles,
    maxWidth: size.width,
    maxHeight: size.height,
    hdrTypes: ["sdr", ...hdr.filter((_, index) => hdrFits[index]).map((x) => x.hdrType)],
  } satisfies VideoCodecSupport;
}

export async function capabilityProfile(): Promise<CapabilityProfile> {
  const highDynamicRange = window.matchMedia("(dynamic-range: high)").matches;
  const videoSupports = await Promise.all(videoCandidates.map((x) => videoSupport(x, highDynamicRange)));
  const videoCodecs: VideoCodecSupport[] = [];
  for (const support of videoSupports) {
    const same = support && videoCodecs.find((x) => x.codec === support.codec);
    if (same) {
      same.profiles.push(...support.profiles);
      same.maxWidth = Math.max(same.maxWidth, support.maxWidth);
      same.maxHeight = Math.max(same.maxHeight, support.maxHeight);
      same.hdrTypes = [...new Set([...same.hdrTypes, ...support.hdrTypes])];
    } else if (support) {
      videoCodecs.push({ ...support, profiles: [...support.profiles] });
    }
  }
  const audioFits = await Promise.all(
    audioCandidates.map((x) =>
      supported({ type: "file", audio: { contentType: x.contentType, channels: "2", bitrate: 192_000, samplerate: 48_000 } }),
    ),
  );
  const audioCodecs: AudioCodecSupport[] = audioCandidates
    .filter((_, index) => audioFits[index])
    .map((x) => ({ codec: x.codec, maxChannels: 2 }));
  return { containers: ["mp4", "webm"], videoCodecs, audioCodecs, subtitleCodecs: [] };
}
