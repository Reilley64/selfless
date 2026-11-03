package dev.reilley.selfless.capabilityprofile

import android.content.Context
import android.hardware.display.DisplayManager
import android.media.MediaCodecInfo
import android.media.MediaCodecInfo.CodecProfileLevel
import android.media.MediaCodecList
import android.os.Build
import android.view.Display
import android.view.Display.HdrCapabilities
import androidx.media3.common.AudioAttributes
import androidx.media3.common.Format
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioCapabilities
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

private class VideoCandidate(
  val codec: String,
  val mimeType: String,
  val profiles: Map<Int, String>,
  val levels: List<Int>,
)

private val avcLevels = listOf(10, 9, 11, 12, 13, 20, 21, 22, 30, 31, 32, 40, 41, 42, 50, 51, 52, 60, 61, 62)

private val hevcLevels = listOf(30, 60, 63, 90, 93, 120, 123, 150, 153, 156, 180, 183, 186)

private val videoCandidates = listOf(
  VideoCandidate(
    "h264",
    MimeTypes.VIDEO_H264,
    mapOf(
      CodecProfileLevel.AVCProfileConstrainedBaseline to "constrainedbaseline",
      CodecProfileLevel.AVCProfileBaseline to "baseline",
      CodecProfileLevel.AVCProfileMain to "main",
      CodecProfileLevel.AVCProfileExtended to "extended",
      CodecProfileLevel.AVCProfileHigh to "high",
      CodecProfileLevel.AVCProfileHigh10 to "high10",
    ),
    avcLevels,
  ),
  VideoCandidate(
    "hevc",
    MimeTypes.VIDEO_H265,
    mapOf(
      CodecProfileLevel.HEVCProfileMain to "main",
      CodecProfileLevel.HEVCProfileMain10 to "main10",
      CodecProfileLevel.HEVCProfileMain10HDR10 to "main10",
      CodecProfileLevel.HEVCProfileMain10HDR10Plus to "main10",
    ),
    hevcLevels.flatMap { listOf(it, it) },
  ),
  VideoCandidate(
    "av1",
    MimeTypes.VIDEO_AV1,
    mapOf(
      CodecProfileLevel.AV1ProfileMain8 to "main",
      CodecProfileLevel.AV1ProfileMain10 to "main",
      CodecProfileLevel.AV1ProfileMain10HDR10 to "main",
      CodecProfileLevel.AV1ProfileMain10HDR10Plus to "main",
    ),
    (0..23).toList(),
  ),
)

private val tenBitProfiles = setOf(
  CodecProfileLevel.AVCProfileHigh10,
  CodecProfileLevel.HEVCProfileMain10,
  CodecProfileLevel.HEVCProfileMain10HDR10,
  CodecProfileLevel.HEVCProfileMain10HDR10Plus,
  CodecProfileLevel.AV1ProfileMain10,
  CodecProfileLevel.AV1ProfileMain10HDR10,
  CodecProfileLevel.AV1ProfileMain10HDR10Plus,
)

private val sizes = listOf(3840 to 2160, 1920 to 1080, 1280 to 720)

private val audioCandidates = listOf(
  "aac" to MimeTypes.AUDIO_AAC,
  "mp3" to MimeTypes.AUDIO_MPEG,
  "flac" to MimeTypes.AUDIO_FLAC,
  "opus" to MimeTypes.AUDIO_OPUS,
  "vorbis" to MimeTypes.AUDIO_VORBIS,
  "alac" to MimeTypes.AUDIO_ALAC,
  "ac3" to MimeTypes.AUDIO_AC3,
  "eac3" to MimeTypes.AUDIO_E_AC3,
  "truehd" to MimeTypes.AUDIO_TRUEHD,
  "dts" to MimeTypes.AUDIO_DTS,
)

private val passthroughChannels = listOf(8, 6, 2)

private val containers = listOf("mkv", "webm", "mp4", "mov")

private val subtitleCodecs = listOf(
  "subrip",
  "ass",
  "ssa",
  "webvtt",
  "mov_text",
  "hdmv_pgs_subtitle",
  "dvd_subtitle",
  "dvb_subtitle",
)

@androidx.annotation.OptIn(UnstableApi::class)
class CapabilityProfileModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("CapabilityProfile")

    AsyncFunction("capabilityProfile") {
      val context = appContext.reactContext ?: throw Exceptions.ReactContextLost()
      val decoders = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { !it.isEncoder }
      val hdrTypes = displayHdrTypes(context)
      val passthrough = AudioCapabilities.getCapabilities(context, AudioAttributes.DEFAULT, null)
      mapOf(
        "containers" to containers,
        "videoCodecs" to videoCandidates.mapNotNull { videoSupport(it, decoders, hdrTypes) },
        "audioCodecs" to audioCandidates.mapNotNull { (codec, mimeType) -> audioSupport(codec, mimeType, decoders, passthrough) },
        "subtitleCodecs" to subtitleCodecs,
      )
    }
  }
}

private fun decodersFor(decoders: List<MediaCodecInfo>, mimeType: String) =
  decoders.mapNotNull { decoder ->
    decoder.supportedTypes.firstOrNull { it.equals(mimeType, ignoreCase = true) }?.let(decoder::getCapabilitiesForType)
  }

private fun displayHdrTypes(context: Context): IntArray {
  val display = context.getSystemService(DisplayManager::class.java).getDisplay(Display.DEFAULT_DISPLAY)
  return if (Build.VERSION.SDK_INT >= 34) display.mode.supportedHdrTypes else display.hdrCapabilities?.supportedHdrTypes ?: intArrayOf()
}

private fun videoSupport(candidate: VideoCandidate, decoders: List<MediaCodecInfo>, displayHdrTypes: IntArray): Map<String, Any>? {
  val capabilities = decodersFor(decoders, candidate.mimeType)
  val profileLevels = capabilities.flatMap { it.profileLevels.asList() }.map { it.profile }
  val profiles = profileLevels.mapNotNull { candidate.profiles[it] }.distinct()
  val size = sizes.firstOrNull { (width, height) ->
    capabilities.any { it.videoCapabilities?.areSizeAndRateSupported(width, height, 30.0) == true }
  }
  if (profiles.isEmpty() || size == null) {
    return null
  }
  val tenBit = profileLevels.any { it in tenBitProfiles }
  val maxLevel = capabilities
    .flatMap { it.profileLevels.asList() }
    .filter { it.profile in candidate.profiles }
    .mapNotNull { candidate.levels.getOrNull(Integer.numberOfTrailingZeros(it.level)) }
    .maxOrNull() ?: 0
  val dolbyVisionProfiles = if (candidate.codec == "hevc" && HdrCapabilities.HDR_TYPE_DOLBY_VISION in displayHdrTypes) {
    decodersFor(decoders, MimeTypes.VIDEO_DOLBY_VISION)
      .flatMap { it.profileLevels.asList() }
      .map { Integer.numberOfTrailingZeros(it.profile) }
      .distinct()
      .sorted()
  } else {
    emptyList()
  }
  val hdrTypes = listOfNotNull(
    "sdr",
    "hdr10".takeIf { tenBit && HdrCapabilities.HDR_TYPE_HDR10 in displayHdrTypes },
    "hlg".takeIf { tenBit && HdrCapabilities.HDR_TYPE_HLG in displayHdrTypes },
    "dolbyVision".takeIf { dolbyVisionProfiles.isNotEmpty() },
  )
  return mapOf(
    "codec" to candidate.codec,
    "profiles" to profiles,
    "maxLevel" to maxLevel,
    "maxWidth" to size.first,
    "maxHeight" to size.second,
    "bitDepths" to if (tenBit) listOf(8, 10) else listOf(8),
    "hdrTypes" to hdrTypes,
    "dolbyVisionProfiles" to dolbyVisionProfiles,
  )
}

@androidx.annotation.OptIn(UnstableApi::class)
private fun audioSupport(
  codec: String,
  mimeType: String,
  decoders: List<MediaCodecInfo>,
  passthrough: AudioCapabilities,
): Map<String, Any>? {
  val decoded = decodersFor(decoders, mimeType).mapNotNull { it.audioCapabilities?.maxInputChannelCount }.maxOrNull()
  val passedThrough = passthroughChannels.firstOrNull { channels ->
    passthrough.isPassthroughPlaybackSupported(
      Format.Builder().setSampleMimeType(mimeType).setChannelCount(channels).setSampleRate(48_000).build(),
      AudioAttributes.DEFAULT,
    )
  }
  val maxChannels = listOfNotNull(decoded, passedThrough).maxOrNull() ?: return null
  return mapOf("codec" to codec, "maxChannels" to maxChannels)
}
