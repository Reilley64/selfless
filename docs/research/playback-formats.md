# Playback formats on Apple TV, Google TV and browsers

Research for [SELFLESS-7](https://app.plane.so/reilley/browse/SELFLESS-7/). Collected 2026-10-29. Versions checked on that date: tvOS 27.2, Media3 1.11.1, expo-video 58.0.13, react-native-video 6.19.3, Chrome 156, Firefox 159, Safari 27.2, FFmpeg 9.0.2, intel/media-driver 26.3.5. Sources are linked by label and listed at the end. "Silent" means the owning source does not cover the point. "Not verified" means nobody tested it on a device.

## Summary

- **One file plays everywhere.** H.264 (8-bit, High profile, level 4.1 or lower) with stereo AAC-LC in MP4 plays directly on Apple TV, Google TV and every current browser. Everything else depends on the client.
- **Apple TV is the strictest client.** AVPlayer plays MP4 and MOV files and HLS. It does not play Matroska (MKV), WebM, DTS, TrueHD or Opus. In HLS it shows only WebVTT, IMSC1 and CEA-608/708 subtitles. Most MKV files from Radarr and Sonarr therefore need at least a remux into HLS on Apple TV. Only the spec page of the Apple TV 4K (4th gen, A19, on sale 2026-11-13) lists AV1, VP9 and Dolby Vision profile 8.1.
- **Google TV is the most capable client.** ExoPlayer reads MKV, MP4, WebM and MPEG-TS. It renders SRT, a subset of ASS, WebVTT, TTML, PGS, VobSub and DVB subtitles. Codec support comes from each device's hardware decoders. The Google TV Streamer decodes H.264, HEVC, VP9 and AV1 up to 4K60, with Dolby Vision, HDR10, HDR10+ and HLG. Its spec page lists Dolby Digital, Dolby Digital Plus and Atmos. Passthrough, DTS and TrueHD depend on the device and the TV or receiver.
- **Browsers differ most from each other.** All play MP4 and WebM with H.264, VP9, AV1, AAC, Opus and FLAC. HEVC works only where the OS or hardware decodes it. AV1 in Safari needs hardware decoding. Only Safari plays AC-3 and E-AC-3. No browser plays DTS or TrueHD. The only subtitle format browsers render natively is WebVTT. ASS and PGS need a JavaScript renderer.
- **When to change the file.** Serve the original file when the client supports its container, every selected stream and the selected subtitle. Remux (copy streams into a new container) when only the container is wrong. Transcode audio alone when only the audio codec is wrong. Convert text subtitles to WebVTT. Transcode video when the codec, profile, level, bit depth or bitrate is wrong, when HDR must become SDR, or when image subtitles (PGS, VobSub) must be burned in for Apple TV or a browser without a renderer.
- **Protocol.** Use HTTP range requests for direct play on all clients. Use HLS with fragmented MP4 (fMP4) segments for remux and transcode on all clients: native on Apple TV and Safari, ExoPlayer's HLS module on Google TV, and hls.js (Apache-2.0) in other browsers. Apple requires fMP4 for HEVC, Dolby Vision and AV1 in HLS, so fMP4 is the one segment format that covers every case.
- **Rust and Quick Sync.** Run the `ffmpeg` and `ffprobe` command-line tools as child processes with `tokio::process`. Read progress from `-progress pipe:1` and decode `ffprobe -of json` with serde. This keeps the server's permissive license separate from FFmpeg's (L)GPL and avoids libav* ABI churn. On the i5-12600H, Quick Sync decodes H.264, HEVC (8, 10 and 12-bit), VP9 and AV1, and encodes H.264, HEVC and VP9. It cannot encode AV1. Tone mapping runs in `vpp_qsv` or `tonemap_vaapi`.

## Client matrix

"Yes" means the client plays the format without server work. "Device" means it depends on the hardware decoder or the HDMI sink and must be checked at runtime.

### Containers

| Container | Apple TV (AVPlayer) | Google TV (ExoPlayer) | Chrome / Edge | Firefox | Safari |
|---|---|---|---|---|---|
| MP4, MOV | Yes [atv-specs-3] [avplayer] | Yes [exo-formats] | Yes [mdn-containers] | Yes | Yes |
| fMP4 | In HLS [hls-spec] | Yes | Yes, also in MSE | Yes, also in MSE | Yes, also in MSE |
| Matroska (MKV) | No [swiftfin-players] | Yes [exo-formats] | Registered from Chrome 145, not verified at runtime [chromium-mime] | Yes since 145, common codecs [ff145] | Silent, treat as no |
| WebM | No [swiftfin-players] | Yes | Yes | Yes | Yes (iOS since 17.4) [safari-17.4] |
| MPEG-TS | In HLS, H.264 only [hls-spec] | Yes | Through hls.js | Through hls.js | In native HLS |

Apple names only MP4, M4V and MOV on its Apple TV spec pages [atv-specs-3]. MediaExtension, the API that lets apps add container readers, is macOS only [mediaextension]. Swiftfin's player documentation lists MKV, MPEG-TS and WebM as unsupported by AVPlayer [swiftfin-players].

### Video codecs

| Codec | Apple TV | Google TV | Chrome / Edge | Firefox | Safari |
|---|---|---|---|---|---|
| H.264 | Yes; HLS on tvOS up to High 5.1 [hls-spec] | Yes; CDD requires High 4.2 at 1080p60 [cdd17] | Yes | Yes (OS decoder, OpenH264 fallback) [mdn-video] | Yes |
| HEVC Main / Main10 | Yes, all models; in HLS only in fMP4 with `hvc1` [hls-spec] | Device; CDD requires Main 4.1, and Main10 level 5 if the device claims UHD [cdd17] | Hardware only (Chrome 107+); Edge needs the paid HEVC extension [chrome-hevc] [mdn-video] | Windows 134+, macOS 136+, Linux 137+ (system ffmpeg), Android hardware only [ff-hevc] | Yes [mdn-video] |
| AV1 | 4th gen only, dedicated decoder [atv-news] [atv-specs-4] | Device; CDD allows a software decoder, so 4K is not guaranteed [cdd17]; Streamer yes to 4K60 [cast-media] | Yes [mdn-video] | Yes | Hardware only (M3 Mac, iPhone 15 Pro and later) [webkit-17] [mdn-video] |
| VP9 | 4th gen spec page only; not allowed in HLS [atv-specs-4] [hls-spec] | Yes when the device has a VP9 hardware decoder [cdd17] | Yes | Yes (MSE off on Android) [ff-prefs] | Yes |
| MPEG-2, VC-1 | No (not listed) | MPEG-2 required [cdd17]; VC-1 device | No | No | No |

### HDR

| Format | Apple TV | Google TV | Browsers |
|---|---|---|---|
| HDR10 | Yes, all models [atv-specs-3] | Device; Streamer and Chromecast 4K yes [gtv-specs] | Chrome tone-maps HDR10 itself [chrome-hdr]; Firefox on macOS 100+ and Windows 153+ [ff153]; Safari yes |
| HLG | 2nd gen and later [atv-specs-2] | Streamer, Chromecast 4K and HD [gtv-specs] | Silent |
| HDR10+ | 3rd and 4th gen [atv-specs-3] [atv-specs-4] | Streamer, Chromecast 4K and HD | Chrome MediaCapabilities returns false [chrome-hdr] |
| Dolby Vision 5 | All models [atv-specs-3] | Streamer, Chromecast 4K (profiles not published) [gtv-specs] | Chrome on Windows only [chromium-gni]; Firefox no [ff-mp4decoder]; Safari silent |
| Dolby Vision 8.1 | 4th gen spec page [atv-specs-4]; HLS signals it on an HDR10 or HLG base layer [hls-spec] | Device | As above |
| Dolby Vision 7 (dual layer) | Apple is silent; Swiftfin says AVPlayer plays the base layer only [swiftfin-players] | Media3 main adds an HEVC fallback for profile 7 (unreleased) [media3-main] | No |

### Audio codecs

| Codec | Apple TV | Google TV | Chrome / Edge | Firefox | Safari |
|---|---|---|---|---|---|
| AAC | Yes | Yes (CDD) [cdd17] | Yes (not in open-source Chromium) | Yes | Yes |
| MP3 | Yes | Yes | Yes | Yes | Yes |
| FLAC | Yes; fMP4 only in HLS [hls-spec] | Yes; Android's platform table lists mono and stereo only [android-formats] | Yes | Yes | Yes |
| Opus | No (not listed) [atv-specs-3] [swiftfin-players] | Yes (Android 5.0+) [android-formats] | Yes | Yes | Yes since 17 (stereo, MP4/WebM) [safari-17] |
| ALAC | Yes | Device | Silent | Silent | Yes |
| AC-3, E-AC-3 | Yes, decoded to PCM or re-encoded to Dolby Digital [atv-audio] | Passthrough when the HDMI sink supports it, otherwise a device decoder if present [audio-caps] | No (Cast and tvOS builds only) [chromium-gni] | No [ff-mp4decoder] | Yes on Apple devices [hls-spec] |
| E-AC-3 JOC (Atmos) | Yes, sent as Dolby MAT [atv-audio] | Passthrough; falls back to the E-AC-3 base layer [audio-caps] | No | No | Yes (HLS) |
| TrueHD | No | Passthrough if the sink reports it [audio-caps]; Google does not publish this for its devices | No | No | No |
| DTS, DTS-HD | No | As TrueHD | No | No | No |

Apple TV does not document bitstream passthrough. Its "Auto" setting decodes audio and sends PCM, "Convert to Dolby Digital 5.1" re-encodes, and Atmos goes out as Dolby MAT [atv-audio]. On Google TV, ExoPlayer chooses passthrough from `AudioCapabilities`, which reads `AudioManager.getDirectProfilesForAttributes` on API 33 and later. Bluetooth output gets PCM only [audio-caps] [tv-audio].

### Subtitles

| Format | Apple TV | Google TV | Browsers |
|---|---|---|---|
| WebVTT | In HLS, with `X-TIMESTAMP-MAP` [hls-spec] | Yes [exo-formats] | Yes, the only native format [mdn-track] |
| IMSC1 / TTML | In HLS, fMP4 only, text profile [hls-spec] | Yes | Through hls.js or Shaka [hlsjs] |
| CEA-608/708 | Yes, inside the video segments [hls-spec] | Yes | Through hls.js |
| tx3g (mov_text) | Core Media type exists [cm-text]; Swiftfin still converts it to WebVTT [swiftfin-avplayer] | Yes | Silent; convert to WebVTT |
| SRT | No; convert to WebVTT | Yes, embedded or sideloaded | Convert to WebVTT |
| ASS / SSA | No; convert to WebVTT (styling lost) or burn in | Partial: alignment, colours, bold and italic, `\pos` and `\move`; other override tags stripped [exo-ssa] | JASSUB (libass in WASM) or ASS.js [jassub] [assjs] |
| PGS, VobSub, DVB | No; burn in | Yes, embedded only [exo-formats] | libpgs or libbitsub (MIT) [libpgs] [libbitsub] |

JASSUB's npm license expression includes LGPL-2.1, FreeType and GPL-2.0 alternatives besides MIT, because the WASM bundles libass, FreeType and other libraries [jassub-npm]. ASS.js is plain MIT but less complete [assjs]. jellyfin-web 12.4 uses `@jellyfin/libass-wasm` and `libbitsub` [jellyfin-web].

## Player libraries in Expo

Both Expo candidates use AVPlayer on tvOS and Media3 ExoPlayer on Android TV. Their APIs limit what the server can hand them.

- **expo-video 58.0.13** supports tvOS (16.4+) and Android [expo-video-docs]. It ships Media3 1.9.1 with HLS and DASH but no SmoothStreaming [expo-video-gradle]. It has no API for sideloaded subtitles. It can only list and select tracks the player already sees (`availableSubtitleTracks`, `availableAudioTracks`) [expo-video-docs]. On Apple TV this means subtitles must arrive as renditions inside the HLS playlist. It renders through AVPlayerViewController, which applies frame-rate and dynamic-range matching automatically in fullscreen [avdisplay]. Not verified on a device.
- **react-native-video 6.19.3** supports tvOS 13+ [rnv-podspec]. Its sidecar `textTracks` and track selection props do not list tvOS, and sidecar tracks do not work with HLS on iOS [rnv-props]. It pins Media3 1.8.0 [rnv-gradle]. Version 7 (beta 12) has no tvOS platform in its podspec yet; PR #5150 adding TV support is open [rnv-pr5150].
- Neither library enables ExoPlayer's FFmpeg audio extension, so Google TV clients can only use the device's decoders and passthrough [rnv-source] [expo-video-source].
- Swiftfin, Jellyfin's tvOS client, works around AVPlayer by using mpv. On its `main` branch mpv is the default player and AVPlayer the fallback [swiftfin-2382]. Its mpv wrapper links a GPL build of libmpv and FFmpeg [mpvui]. KSPlayer is GPL-3.0 unless a paid license is bought, and VLCKit is LGPL-2.1 [ksplayer] [vlckit]. Each of these would bring copyleft terms into the TV app. An AVPlayer-only Apple TV app keeps the server doing the remux and transcode work instead.

## When the server must remux or transcode

The server decides per playback request. It compares the file's streams from ffprobe with the client's capabilities. Jellyfin does this in `StreamBuilder.GetOptimalVideoStream`, which compares a client device profile with each stream and returns direct play, direct stream (remux) or transcode with reason flags [jf-streambuilder]. selfless can follow the same idea in its own code.

The client should report its capabilities instead of the server guessing from a device name. Apple TV can ask `VTIsHardwareDecodeSupported` for each codec [vt-hw] and `AVURLAsset.audiovisualMIMETypes()` for containers [avurlasset]. Android can query `MediaCodecList` and `AudioManager.getDirectProfilesForAttributes` [tv-audio]. Browsers can call `MediaCapabilities.decodingInfo()` and `MediaSource.isTypeSupported()` [mediacap].

| Case | Server action | Cost |
|---|---|---|
| Container, all selected streams and subtitle supported | Direct play over HTTP range requests | None |
| Container unsupported (MKV on Apple TV and Safari) | Remux video and audio with `-c copy` into HLS fMP4 | Low CPU; segments must start on source keyframes |
| Audio codec unsupported (DTS or TrueHD anywhere except some Google TV sinks; AC-3 or E-AC-3 in Chrome and Firefox; Opus on Apple TV) | Copy video, encode audio to AAC stereo, and to AC-3 or E-AC-3 for 5.1 on TV clients | Low; FFmpeg has native `aac`, `ac3` and `eac3` encoders [ffmpeg-codecs] [ffmpeg-allcodecs] |
| Text subtitle the client cannot show (SRT, ASS, mov_text on Apple TV and browsers) | Convert to WebVTT and add it as an HLS subtitle rendition or `<track>` | Low; ASS styling is lost |
| Image subtitle the client cannot show (PGS, VobSub on Apple TV; browsers without libpgs) | Burn into the video | Forces a video transcode |
| Video codec, profile, level or bit depth unsupported (AV1 on Apple TV before 4th gen; HEVC in Firefox without a decoder; VC-1 and MPEG-2 outside Google TV) | Transcode video to H.264 or HEVC | Quick Sync |
| HDR source on an SDR display, or Dolby Vision profile 7 or 5 on a client that lacks it | Tone-map to SDR while transcoding | Quick Sync with VPP |
| Bitrate above the user's network limit | Transcode to a lower bitrate | Quick Sync |
| Interlaced video on Apple TV | Deinterlace (Apple requires it in HLS) [hls-spec] | Quick Sync |

Dolby Vision profile 5 has no HDR10 base layer, so a client without Dolby Vision gets wrong colours from it. It always needs tone mapping on such clients. This follows from the profile definitions; the sources above do not test it.

## Streaming protocol per client

| Client | Direct play | Remux and transcode | Notes |
|---|---|---|---|
| Apple TV | Progressive MP4 or MOV over HTTP range requests [avplayer] [avplayer-log] | HLS, fMP4 segments | Native AVPlayer. Required for MKV sources and for subtitles with expo-video |
| Google TV | Progressive MKV, MP4 or WebM | HLS, fMP4 segments | ExoPlayer HLS supports fMP4 and TS, WebVTT and CEA captions [exo-formats] |
| Safari (macOS, iOS) | Progressive MP4 | Native HLS | ManagedMediaSource lets hls.js run on iOS 17.1+ [safari-17.1] |
| Chrome, Edge | Progressive MP4 or WebM | hls.js on MSE | Chrome has native HLS on desktop since 143 (on by default) [chromium-switches], but `canPlayType` for HLS accepts only H.264, AAC and MP3 [chromium-mime]. hls.js warns that Chrome may report support and still fail [hlsjs] |
| Firefox | Progressive MP4 or WebM | hls.js on MSE | No native HLS on desktop [ff-prefs] |

Recommendation: one adaptive protocol, HLS version 7 with fMP4 segments, for every non-direct session. DASH adds nothing here. Apple TV does not play it, and expo-video plays it only on Android [expo-video-types]. fMP4 covers HEVC, Dolby Vision and AV1, which Apple allows only in fMP4 [hls-spec]. hls.js passes fMP4 to MSE without remuxing [hlsjs].

Apple's HLS authoring spec has App Store rules that a home server may not need. Examples are several bitrates, I-frame playlists, gzip on playlists and a 6 second target duration [hls-spec]. Which rules AVPlayer enforces at runtime is not documented. 
Jellyfin's design for on-demand HLS is worth reading for ideas (GPL, no code copied) [jf-dynamichls]:

- The server writes the full VOD playlist before FFmpeg starts. Transcode segments have equal length. Remux segments follow the source keyframes [jf-hls-playlist].
- A segment request serves the file if it exists. FFmpeg restarts at the requested segment with `-start_number` if the request is behind the encoder or more than about 24 seconds ahead.
- A throttler pauses FFmpeg when it is far ahead of the client. This uses a pause key that only jellyfin-ffmpeg adds (patch 0028) [jf-throttler] [jf-ffmpeg-patches].
- A cleaner deletes served segments [jf-cleaner].

## Driving FFmpeg with Quick Sync from Rust

### What the i5-12600H can do

The i5-12600H is Alder Lake-P. intel/media-driver groups it with Tiger Lake under "TGLx" [media-driver].

| Codec | Decode | Encode |
|---|---|---|
| H.264 | Yes | Yes, low-power (VDEnc) and shader paths |
| HEVC 8 and 10-bit 4:2:0 | Yes | Yes |
| HEVC 8 and 10-bit 4:4:4 | Yes | Yes |
| HEVC 8 and 10-bit 4:2:2 | Yes | Shader path only, full-feature driver only |
| HEVC 12-bit | Yes | No (4:2:0 shader path only) |
| VP9 8 and 10-bit | Yes | Yes |
| AV1 8 and 10-bit | Yes (full-feature driver only) | No; Intel adds AV1 encode with DG2 (Arc) |
| MPEG-2 | Yes | Yes (shader) |
| VC-1 | Yes (full-feature driver only) | No |

- HDR10 tone mapping in the video engine needs the full-feature driver package, `intel-media-va-driver-non-free` (Debian non-free) [media-driver].
- Low-power encoding needs HuC firmware for bitrate control. On ADL-P the i915 kernel driver loads GuC and HuC by default from Linux 5.17. Do not set `enable_guc` on this host [media-driver] [i915-uc] [jf-intel].
- Alder Lake needs the oneVPL runtime (`libmfx-gen`, dispatcher `libvpl`). The legacy Media SDK does not cover it and is discontinued [libvpl] [mediasdk]. `vpp_qsv` tone mapping compiles only in libvpl builds of FFmpeg [vf-vpp-qsv].
- An Intel iGPU has no limit on concurrent encode sessions [jf-intel].
- The homelab Jellyfin on the same host uses QSV low-power H.264 and HEVC encoding with VPP tone mapping. It transcoded 4K Dolby Vision and HDR10 to 1080p SDR at about 2.9 times real time (homelab notes, 2026-10-21). That is one measurement, not a benchmark.

### FFmpeg command shape

Device setup as Jellyfin does it on Linux. It derives the QSV device from a VAAPI device [jf-encodinghelper] [ffmpeg-doc]:

```sh
ffmpeg -init_hw_device vaapi=va:/dev/dri/renderD128,driver=iHD \
       -init_hw_device qsv=qs@va -filter_hw_device qs \
       -hwaccel qsv -hwaccel_output_format qsv -c:v hevc_qsv -i input.mkv \
       -vf 'vpp_qsv=w=1920:h=1080:format=nv12:tonemap=1' \
       -c:v h264_qsv -low_power 1 -b:v 8M -maxrate 8M -g 144 -keyint_min 144 \
       -c:a aac -ac 2 -b:a 192k \
       -f hls -hls_segment_type fmp4 -hls_time 6 -hls_playlist_type vod \
       -start_number 0 -copyts -avoid_negative_ts disabled \
       -hls_segment_filename 'seg%d.m4s' out.m3u8
```

- `-g` and `-keyint_min` set the GOP to segment length times frame rate (here 6 s at 24 fps). Jellyfin does this for QSV encoders because `-force_key_frames` does not work with them [jf-encodinghelper].
- The HLS muxer cuts a segment at the next keyframe after `hls_time`. With `-c:v copy`, cuts can only fall on source keyframes [ffmpeg-formats].
- `tonemap_vaapi` is the other tone-mapping path. It accepts only HDR10 input, and upstream fails when mastering metadata is missing; jellyfin-ffmpeg patches that [ffmpeg-filters] [jf-ffmpeg-patch-0070].
- `-progress pipe:1` writes `key=value` blocks that end with `progress=continue` or `progress=end` [ffmpeg-doc]. `ffprobe -of json -show_streams -show_format` gives stream metadata [ffprobe-doc].
- The FFmpeg docs do not document `vpp_qsv` and `scale_qsv` on the filters page. Their options come from the source file [vf-vpp-qsv] and the QuickSync wiki [ffmpeg-qsv-wiki].

### Rust options

| Option | Latest (crates.io) | License | Fit |
|---|---|---|---|
| `tokio::process` running the CLI | tokio 1.54.1 | MIT | Recommended. No extra crate. Set `kill_on_drop(true)`, which defaults to false [tokio-command] |
| ffmpeg-sidecar | 2.7.0 (2026-10-18) | MIT | CLI wrapper with progress parsing. Synchronous API; its auto-download fetches GPL builds [ffmpeg-sidecar] |
| ffprobe | 0.4.0 (2024-04-03) | MIT | Typed ffprobe JSON. Stale; plain serde types are enough [ffprobe-rs] |
| ffmpeg-next | 9.0.0 (2026-08-05) | WTFPL | Links libav*. In maintenance mode [ffmpeg-next] |
| ffmpeg-the-third | 6.0.0+ffmpeg-9.0 | WTFPL | Active fork of ffmpeg-next, FFmpeg 5.1 to 9.0 [ffmpeg-the-third] |
| rsmpeg | 0.18.0+ffmpeg.8.0 (2025-08-24) | MIT | Links libav*. No FFmpeg 9.0 release [rsmpeg] |
| ez-ffmpeg | 0.19.3 | MIT, Apache-2.0 or MPL-2.0 | Links libav*, FFmpeg 7.1 to 8.x [ez-ffmpeg] |

Recommendation: spawn the CLI. Three facts support this:

- FFmpeg 9.0 bumped every library's major version, so linked bindings break each release [ffmpeg-download].
- The bindings either lag FFmpeg or are in maintenance mode (table above).
- Linking libav* brings LGPL terms (dynamic linking, source offer, no GPL parts). Linking a GPL build such as one with libx264 makes the whole server GPL [ffmpeg-legal]. A separate process is a separate program under the FSF's GPL FAQ, unless the two exchange intimate internal data structures [gpl-faq].

With the CLI, each FFmpeg process can map to one OpenTelemetry span.

### FFmpeg binary and container

- **jellyfin-ffmpeg 8.1.3-1** is GPLv3. It has the QSV fixes, the pause key, tone mapping without HDR10 metadata and Dolby Vision side data in the HLS muxer [jf-ffmpeg-releases] [jf-ffmpeg-rules] [jf-ffmpeg-patches]. It also builds libfdk-aac without `--enable-nonfree`, which upstream calls incompatible with the GPL [jf-ffmpeg-patch-0026] [ffmpeg-license]. That matters only if selfless redistributes an image containing it.
- **Debian ffmpeg** is GPL with libvpl on amd64. Trixie has 7.1.5, forky has 9.0.2 [debian-pkgs] [debian-ffmpeg-rules].
- Distributing any FFmpeg binary in a Docker image carries that binary's license duties, but not for the server code [gpl-faq].
- In Docker, pass `/dev/dri/renderD128` and add the host's `render` group id with `group_add` [jf-intel] [docker-run]. Install `intel-media-va-driver-non-free`, `libvpl2` and `libmfx-gen1.2`, and check with `vainfo` [media-driver] [debian-pkgs].

## Open questions for the grilling ticket

1. **Apple TV player.** Use AVPlayer only (expo-video or react-native-video) and let the server remux and transcode? Or add mpv or VLCKit for direct MKV, ASS and PGS playback, and accept GPL or LGPL terms in the TV app?
2. **Expo player library.** expo-video lacks sideloaded subtitles. react-native-video 6 lacks tvOS subtitle selection, and its version 7 lacks tvOS. Which library, and does the server always put subtitles into the HLS playlist?
3. **Capability source.** Should clients send a capability profile with each playback request, or should the server keep per-device profiles like Jellyfin?
4. **Direct play on Apple TV.** Serve MP4 files progressively, or always wrap them in HLS so subtitles and audio tracks behave the same as in remux sessions?
5. **Transcode targets.** H.264 only, or HEVC for Apple TV and Google TV clients that support it? AV1 encode is not available on this CPU.
6. **Audio for TV clients.** Encode multichannel AAC, AC-3 or E-AC-3 when DTS or TrueHD cannot pass through? Does the Google TV Streamer pass through TrueHD and DTS? Google does not say.
7. **Subtitles in browsers.** Render ASS with JASSUB, whose WASM carries LGPL and FreeType terms, or with ASS.js (MIT, less complete)? Render PGS with libbitsub, or burn it in?
8. **Dolby Vision.** Which clients get Dolby Vision as is, which get HDR10 from the base layer, and which get tone-mapped SDR? Profile 7 and 8.1 behaviour on the 3rd gen Apple TV is not documented.
9. **FFmpeg build.** jellyfin-ffmpeg, Debian, or an own build? Throttling FFmpeg needs the jellyfin-ffmpeg pause patch, or another way such as pausing with SIGSTOP or not throttling.
10. **Session model.** Copy Jellyfin's idea of a full VOD playlist with segments made on demand and FFmpeg restarts on seek, or transcode whole files ahead of time?
11. **Device checks.** Several facts need a test on real hardware: Chrome 145+ MKV playback, the ASS styling ExoPlayer drops, expo-video frame-rate matching, and which HLS rules AVPlayer enforces.

## Sources

Apple
- [atv-news] Apple Newsroom, "Apple unveils the new Apple TV 4K", 2026-10: https://www.apple.com/newsroom/2026/10/apple-unveils-the-new-apple-tv-4k/
- [atv-specs-4] Apple TV 4K (4th gen) specs: https://www.apple.com/apple-tv-4k/specs/
- [atv-specs-3] Apple TV 4K (3rd gen) tech specs: https://support.apple.com/en-us/111839
- [atv-specs-2] Apple TV 4K (2nd gen) tech specs: https://support.apple.com/en-us/111922
- [atv-audio] Apple TV User Guide, change audio settings (tvOS 27): https://support.apple.com/guide/tv/change-audio-settings-atvba773c3c9/tvos
- [hls-spec] HLS Authoring Specification for Apple Devices, revision 2025-06-26: https://developer.apple.com/streaming/hls-authoring-specification-for-apple-devices.html
- [avplayer] AVPlayer: https://developer.apple.com/documentation/avfoundation/avplayer
- [avplayer-log] AVPlayerItemAccessLogEvent.numberOfMediaRequests: https://developer.apple.com/documentation/avfoundation/avplayeritemaccesslogevent/numberofmediarequests
- [avurlasset] AVURLAsset.audiovisualMIMETypes(): https://developer.apple.com/documentation/avfoundation/avurlasset/audiovisualmimetypes()
- [avdisplay] AVPlayerViewController.appliesPreferredDisplayCriteriaAutomatically: https://developer.apple.com/documentation/avkit/avplayerviewcontroller/appliespreferreddisplaycriteriaautomatically
- [vt-hw] VTIsHardwareDecodeSupported: https://developer.apple.com/documentation/videotoolbox/vtishardwaredecodesupported(_:)
- [cm-text] CMTextFormatType: https://developer.apple.com/documentation/coremedia/cmtextformattype
- [mediaextension] MediaExtension: https://developer.apple.com/documentation/mediaextension
- [safari-17] Safari 17 release notes: https://developer.apple.com/documentation/safari-release-notes/safari-17-release-notes
- [safari-17.1] Safari 17.1 release notes: https://developer.apple.com/documentation/safari-release-notes/safari-17_1-release-notes
- [safari-17.4] Safari 17.4 release notes: https://developer.apple.com/documentation/safari-release-notes/safari-17_4-release-notes
- [webkit-17] WebKit features in Safari 17.0: https://webkit.org/blog/14445/webkit-features-in-safari-17-0/

Google and Android
- [exo-formats] ExoPlayer supported formats, updated 2026-07-21: https://developer.android.com/media/media3/exoplayer/supported-formats
- [exo-ssa] Media3 SsaParser and SsaStyle: https://github.com/androidx/media/blob/release/libraries/extractor/src/main/java/androidx/media3/extractor/text/ssa/SsaParser.java
- [media3-main] Media3 release notes, main branch: https://github.com/androidx/media/blob/main/RELEASENOTES.md
- [audio-caps] Media3 AudioCapabilities: https://github.com/androidx/media/blob/release/libraries/exoplayer/src/main/java/androidx/media3/exoplayer/audio/AudioCapabilities.java
- [tv-audio] Android TV audio capabilities, updated 2026-10-28: https://developer.android.com/training/tv/playback/audio-capabilities
- [android-formats] Android supported media formats, updated 2026-10-07: https://developer.android.com/media/platform/supported-formats
- [cdd17] Android 17 CDD, section 2.3.2 and 5.3: https://source.android.com/docs/compatibility/17/android-17-cdd
- [gtv-specs] Chromecast and Google TV Streamer specifications: https://support.google.com/chromecast/answer/3046409?hl=en
- [cast-media] Google Cast supported media, updated 2024-09-18: https://developers.google.com/cast/docs/media

Browsers
- [mdn-containers] MDN media container formats: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Containers
- [mdn-video] MDN web video codec guide: https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Video_codecs
- [mdn-track] MDN `<track>`: https://developer.mozilla.org/en-US/docs/Web/HTML/Element/track
- [mediacap] W3C Media Capabilities, working draft 2026-10-18: https://www.w3.org/TR/media-capabilities/
- [chrome-hevc] Chrome Platform Status, HEVC hardware decoding: https://chromestatus.com/feature/5186511939567616
- [chrome-hdr] Chrome Platform Status, MediaCapabilities HDR: https://chromestatus.com/feature/6640863931269120
- [chromium-mime] Chromium mime_util_internal.cc at 156: https://chromium.googlesource.com/chromium/src/+/156.0.8078.82/media/base/mime_util_internal.cc
- [chromium-gni] Chromium media_options.gni at 156: https://chromium.googlesource.com/chromium/src/+/156.0.8078.82/media/media_options.gni
- [chromium-switches] Chromium media_switches.cc at 143: https://chromium.googlesource.com/chromium/src/+/143.0.7499.205/media/base/media_switches.cc
- [ff145] Firefox 145 release notes: https://www.firefox.com/en-US/firefox/145.0/releasenotes/
- [ff-hevc] Firefox 134, 136 and 137 release notes: https://www.firefox.com/en-US/firefox/134.0/releasenotes/ , https://www.firefox.com/en-US/firefox/136.0/releasenotes/ , https://www.firefox.com/en-US/firefox/137.0/releasenotes/
- [ff153] Firefox 153 release notes: https://www.firefox.com/en-US/firefox/153.0/releasenotes/
- [ff-mp4decoder] Firefox MP4Decoder.cpp: https://github.com/mozilla-firefox/firefox/blob/main/dom/media/mp4/MP4Decoder.cpp
- [ff-prefs] Firefox StaticPrefList.yaml: https://github.com/mozilla-firefox/firefox/blob/main/modules/libpref/init/StaticPrefList.yaml
- [hlsjs] hls.js README, v1.7.4: https://github.com/video-dev/hls.js/blob/master/README.md
- [jassub] JASSUB: https://github.com/ThaUnknown/jassub
- [jassub-npm] jassub on npm, 2.5.23: https://registry.npmjs.org/jassub
- [assjs] ASS.js: https://github.com/weizhenye/ASS
- [libpgs] libpgs-js: https://github.com/Arcus92/libpgs-js
- [libbitsub] libbitsub: https://github.com/altqx/libbitsub
- [jellyfin-web] jellyfin-web package.json: https://github.com/jellyfin/jellyfin-web/blob/master/package.json

Player libraries
- [expo-video-docs] expo-video docs: https://docs.expo.dev/versions/latest/sdk/video/
- [expo-video-types] expo-video VideoPlayer.types.ts: https://github.com/expo/expo/blob/main/packages/expo-video/src/VideoPlayer.types.ts
- [expo-video-gradle] expo-video 58.0.13 build.gradle: https://unpkg.com/expo-video@58.0.13/android/build.gradle
- [expo-video-source] expo-video 58.0.13 package source: https://unpkg.com/expo-video@58.0.13/?meta
- [rnv-podspec] react-native-video 6.19.3 podspec: https://github.com/TheWidlarzGroup/react-native-video/blob/v6.19.3/react-native-video.podspec
- [rnv-props] react-native-video 6 props: https://github.com/TheWidlarzGroup/react-native-video/blob/master/docs/versioned_docs/version-6.x/component/props.mdx
- [rnv-gradle] react-native-video 6.19.3 gradle.properties: https://github.com/TheWidlarzGroup/react-native-video/blob/v6.19.3/android/gradle.properties
- [rnv-source] react-native-video 6.19.3 ReactExoplayerView.java: https://github.com/TheWidlarzGroup/react-native-video/blob/v6.19.3/android/src/main/java/com/brentvatne/exoplayer/ReactExoplayerView.java
- [rnv-pr5150] react-native-video PR #5150: https://github.com/TheWidlarzGroup/react-native-video/pull/5150
- [swiftfin-players] Swiftfin player documentation: https://github.com/jellyfin/Swiftfin/blob/main/Documentation/players.md
- [swiftfin-avplayer] Swiftfin AVPlayer profile: https://github.com/jellyfin/Swiftfin/blob/main/Shared/Objects/MediaPlayerType/MediaPlayerType%2BAVPlayer.swift
- [swiftfin-2382] Swiftfin PR #2382: https://github.com/jellyfin/Swiftfin/pull/2382
- [mpvui] MPVUI Package.swift: https://github.com/LePips/MPVUI/blob/main/Package.swift
- [ksplayer] KSPlayer: https://github.com/kingslay/KSPlayer
- [vlckit] VLCKit: https://github.com/videolan/vlckit

Intel, FFmpeg and Rust
- [media-driver] intel/media-driver README, 26.3.5: https://github.com/intel/media-driver/blob/master/README.md
- [i915-uc] Linux i915 intel_uc.c: https://raw.githubusercontent.com/torvalds/linux/master/drivers/gpu/drm/i915/gt/uc/intel_uc.c
- [libvpl] intel/libvpl README: https://github.com/intel/libvpl/blob/main/README.md
- [mediasdk] Intel Media SDK README: https://github.com/Intel-Media-SDK/MediaSDK/blob/master/README.rst
- [ffmpeg-download] FFmpeg download page (9.0.2, 8.1.3): https://ffmpeg.org/download.html
- [ffmpeg-doc] ffmpeg documentation: https://ffmpeg.org/ffmpeg.html
- [ffprobe-doc] ffprobe documentation: https://ffmpeg.org/ffprobe.html
- [ffmpeg-formats] FFmpeg formats, HLS muxer: https://ffmpeg.org/ffmpeg-formats.html#hls-2
- [ffmpeg-codecs] FFmpeg codecs: https://ffmpeg.org/ffmpeg-codecs.html
- [ffmpeg-allcodecs] FFmpeg allcodecs.c, release/9.0: https://raw.githubusercontent.com/FFmpeg/FFmpeg/release/9.0/libavcodec/allcodecs.c
- [ffmpeg-filters] FFmpeg filters, tonemap_vaapi: https://ffmpeg.org/ffmpeg-filters.html#tonemap_005fvaapi
- [vf-vpp-qsv] FFmpeg vf_vpp_qsv.c, release/9.0: https://raw.githubusercontent.com/FFmpeg/FFmpeg/release/9.0/libavfilter/vf_vpp_qsv.c
- [ffmpeg-qsv-wiki] FFmpeg wiki, QuickSync: https://trac.ffmpeg.org/wiki/Hardware/QuickSync
- [ffmpeg-legal] FFmpeg legal: https://ffmpeg.org/legal.html
- [ffmpeg-license] FFmpeg LICENSE.md, release/9.0: https://raw.githubusercontent.com/FFmpeg/FFmpeg/release/9.0/LICENSE.md
- [gpl-faq] FSF GPL FAQ, mere aggregation: https://www.gnu.org/licenses/gpl-faq.html#MereAggregation
- [tokio-command] tokio::process::Command: https://docs.rs/tokio/latest/tokio/process/struct.Command.html
- [ffmpeg-sidecar] ffmpeg-sidecar: https://github.com/nathanbabcock/ffmpeg-sidecar
- [ffprobe-rs] ffprobe crate: https://github.com/theduke/ffprobe-rs
- [ffmpeg-next] ffmpeg-next: https://github.com/zmwangx/rust-ffmpeg
- [ffmpeg-the-third] ffmpeg-the-third: https://github.com/shssoichiro/ffmpeg-the-third
- [rsmpeg] rsmpeg: https://github.com/larksuite/rsmpeg
- [ez-ffmpeg] ez-ffmpeg: https://github.com/YeautyYE/ez-ffmpeg
- Crate versions, dates and licenses: https://crates.io/api/v1/crates/<name>
- [jf-ffmpeg-releases] jellyfin-ffmpeg releases: https://github.com/jellyfin/jellyfin-ffmpeg/releases
- [jf-ffmpeg-rules] jellyfin-ffmpeg debian/rules: https://github.com/jellyfin/jellyfin-ffmpeg/blob/jellyfin/debian/rules
- [jf-ffmpeg-patches] jellyfin-ffmpeg patch series: https://github.com/jellyfin/jellyfin-ffmpeg/blob/jellyfin/debian/patches/series
- [jf-ffmpeg-patch-0026] jellyfin-ffmpeg patch 0026: https://github.com/jellyfin/jellyfin-ffmpeg/blob/jellyfin/debian/patches/0026-remove-fdk-aac-from-nonfree.patch
- [jf-ffmpeg-patch-0070] jellyfin-ffmpeg patch 0070: https://github.com/jellyfin/jellyfin-ffmpeg/blob/jellyfin/debian/patches/0070-allow-vpp-tonemap-without-hdr10-metadata.patch
- [debian-pkgs] Debian package search: https://packages.debian.org
- [debian-ffmpeg-rules] Debian ffmpeg debian/rules: https://salsa.debian.org/multimedia-team/ffmpeg/-/raw/debian/master/debian/rules
- [docker-run] docker container run reference: https://docs.docker.com/reference/cli/docker/container/run/
- [jf-intel] Jellyfin docs, Intel hardware acceleration: https://jellyfin.org/docs/general/post-install/transcoding/hardware-acceleration/intel/

Jellyfin server (GPL-2.0, read for ideas only, master branch on 2026-10-29)
- [jf-streambuilder] MediaBrowser.Model/Dlna/StreamBuilder.cs: https://github.com/jellyfin/jellyfin/blob/master/MediaBrowser.Model/Dlna/StreamBuilder.cs
- [jf-dynamichls] Jellyfin.Api/Controllers/DynamicHlsController.cs: https://github.com/jellyfin/jellyfin/blob/master/Jellyfin.Api/Controllers/DynamicHlsController.cs
- [jf-hls-playlist] src/Jellyfin.MediaEncoding.Hls/Playlist/DynamicHlsPlaylistGenerator.cs: https://github.com/jellyfin/jellyfin/blob/master/src/Jellyfin.MediaEncoding.Hls/Playlist/DynamicHlsPlaylistGenerator.cs
- [jf-encodinghelper] MediaBrowser.Controller/MediaEncoding/EncodingHelper.cs: https://github.com/jellyfin/jellyfin/blob/master/MediaBrowser.Controller/MediaEncoding/EncodingHelper.cs
- [jf-throttler] MediaBrowser.Controller/MediaEncoding/TranscodingThrottler.cs: https://github.com/jellyfin/jellyfin/blob/master/MediaBrowser.Controller/MediaEncoding/TranscodingThrottler.cs
- [jf-cleaner] MediaBrowser.Controller/MediaEncoding/TranscodingSegmentCleaner.cs: https://github.com/jellyfin/jellyfin/blob/master/MediaBrowser.Controller/MediaEncoding/TranscodingSegmentCleaner.cs
