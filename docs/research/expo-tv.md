# Expo on Apple TV and Google TV

Research for [SELFLESS-6](https://app.plane.so/reilley/browse/SELFLESS-6/). Checked on 2026-10-29. Versions and dates below are as of that day.

## Summary

Yes. The current Expo SDK (58, released 2026-10-15) builds Apple TV and Google TV apps when the `react-native` dependency points at the `react-native-tvos` fork and the `@react-native-tvos/config-tv` plugin runs during prebuild. One codebase gives a tvOS build and an Android TV build. Phone and TV builds come from separate prebuilds of the same source.

- **Video.** `expo-video` is on Expo's list of TV-supported libraries. It wraps AVPlayer on tvOS and Media3 ExoPlayer (with its HLS module) on Android, so HLS plays on both. Its docs list audio and subtitle track selection for Android and iOS only. The tvOS source does not exclude those APIs, so they probably work, but a prototype has to confirm it. `react-native-video` v6 builds for tvOS. v7 lists TV support as a to-do.
- **Focus.** The TV fork ships a native focus engine. Touchables and `Pressable` get focus and blur events, and `TVFocusGuideView`, `nextFocus*` props and `useTVEventHandler` handle layout and remote input. Two JavaScript spatial navigation libraries exist (Norigin, BAM). They are only worth it to share focus code with a web TV target.
- **Telemetry.** The OpenTelemetry JS SDK officially supports React Native and Expo through its browser packages. It is plain JavaScript, so it runs on both TVs and can export OTLP to the homelab collector. It does not catch native crashes. Of the native SDKs checked, Sentry and Datadog declare tvOS in their podspecs. Embrace and Grafana Faro declare iOS only.
- **Installing.** Apple TV needs the paid Apple Developer Program ($99 a year). The usable household paths are TestFlight (each build lasts 90 days), ad hoc builds for registered Apple TVs, or an unlisted App Store app that has to pass App Review. Google TV takes a sideloaded APK through `adb` or an "install unknown apps" source. Android developer verification does not cover `adb` installs, and outside Google Play it is enforced only on phones and tablets in four countries for now.

The main risk is lag. The TV fork and Expo's TV docs trail the core release by days to weeks. On 2026-10-29, Expo's compatibility table still maps SDK 58 to `0.87-stable`, and Expo's own TV examples are still on SDK 57.

## Versions on 2026-10-29

| Package | Version | Date | Source |
| --- | --- | --- | --- |
| `expo` (latest) | 58.0.21, bundles `react-native` 0.88.1 | 2026-10-28 | [npm](https://registry.npmjs.org/expo), [bundledNativeModules.json](https://unpkg.com/expo@58.0.21/bundledNativeModules.json) |
| `react-native-tvos` (latest, `0.88-stable`) | 0.88.0-0 | 2026-10-17 | [npm](https://registry.npmjs.org/react-native-tvos), [release](https://github.com/react-native-tvos/react-native-tvos/releases/tag/v0.88.0-0) |
| `react-native-tvos` `0.86-stable` | 0.86.3-0 | 2026-09-17 | [release](https://github.com/react-native-tvos/react-native-tvos/releases/tag/v0.86.3-0) |
| `@react-native-tvos/config-tv` | 0.1.7 | 2026-09-29 | [npm](https://registry.npmjs.org/@react-native-tvos%2Fconfig-tv) |
| `expo-video` | 58.0.13 | 2026-10-28 | [npm](https://registry.npmjs.org/expo-video) |
| `react-native-video` | 6.19.3 (latest), 7.0.0-beta.12 (beta) | 2026-09-18, 2026-10-06 | [npm](https://registry.npmjs.org/react-native-video) |

Expo's [SDK compatibility table](https://docs.expo.dev/versions/latest/#each-expo-sdk-version-depends-on-a-react-native-version) lists:

| Expo SDK | React Native | React Native TV |
| --- | --- | --- |
| 58 | 0.88 | 0.87-stable |
| 57 | 0.86 | 0.86-stable |
| 56 | 0.85 | 0.85-stable |

The SDK 58 row looks stale. SDK 58 ships React Native 0.88 ([SDK 58 changelog](https://expo.dev/changelog/sdk-58)), the TV guide says the TV version must match the SDK's React Native version ([TV guide](https://docs.expo.dev/guides/building-for-tv/)), and `react-native-tvos` 0.88.0-0 came out two days after core 0.88.0. For 0.83 through 0.88, each TV release followed the core release by 2 to 5 days (publish dates on [npm](https://registry.npmjs.org/react-native-tvos) and [npm](https://registry.npmjs.org/react-native)). Expo's [with-tv](https://github.com/expo/examples/tree/master/with-tv) and [with-router-tv](https://github.com/expo/examples/tree/master/with-router-tv) examples still pin `expo ~57.0.13` with `react-native-tvos@0.86-stable`. Their last update was on 2026-08-17 ([commits](https://github.com/expo/examples/commits/master/with-router-tv)).

## How the TV build works

- Change `react-native` in `package.json` to `npm:react-native-tvos@<x>-stable`, matching the SDK's React Native minor version. From SDK 56, `npx expo install expo@latest --fix` also upgrades the TV dependency. SDK 55 and earlier need a manual bump ([TV guide](https://docs.expo.dev/guides/building-for-tv/)).
- Add `@react-native-tvos/config-tv`. It acts when `EXPO_TV=1` is set or the plugin option `isTV` is true. On Android it removes the portrait orientation and adds the TV launcher intent. On iOS it switches the Podfile and Xcode project to tvOS and fixes the splash storyboard ([TV guide](https://docs.expo.dev/guides/building-for-tv/)). It also takes Android TV banner and icon paths, Apple TV icon and top shelf images, and an `androidTVRequired` flag that makes the app TV-only on Android ([plugin README](https://github.com/react-native-tvos/config-tv/tree/main/packages/config-tv)).
- A React Native Podfile can only have one target, either iOS or tvOS ([fork README](https://github.com/react-native-tvos/react-native-tvos#build-changes)). Running `npx expo prebuild --clean` with or without `EXPO_TV` switches between phone and TV. EAS Build profiles can set `EXPO_TV: "1"` to build TV from the same source ([TV guide](https://docs.expo.dev/guides/building-for-tv/)).
- In a monorepo, every Expo project should use the TV fork if any one of them targets TV. This avoids dependency conflicts ([TV guide](https://docs.expo.dev/guides/building-for-tv/)). This only affects React Native projects. A plain React web app does not depend on `react-native`.
- Hermes and the New Architecture work on both TV platforms ([fork README](https://github.com/react-native-tvos/react-native-tvos)). Since SDK 56, the minimum tvOS version is 16.4 ([SDK 56 changelog](https://expo.dev/changelog/sdk-56)). Android needs API 24 or later ([fork README](https://github.com/react-native-tvos/react-native-tvos)).
- Expo Go does not run TV apps ([plugin README](https://github.com/react-native-tvos/config-tv/tree/main/packages/config-tv)). `expo-dev-client` works on TV from SDK 54. On Apple TV it supports a local or tunneled Metro server only, without EAS login or build lists ([TV guide](https://docs.expo.dev/guides/building-for-tv/)). The plugin README still says `expo-dev-client` and `expo-router` are unsupported. That text dates from SDK 50, and Expo's guide and its `with-router-tv` example contradict it.
- Expo lists the libraries that work on TV. The list includes Video, Image, SecureStore, SQLite, Updates, NetInfo, FlashList, Reanimated and Localization. It also says React Navigation works ([TV guide](https://docs.expo.dev/guides/building-for-tv/#see-which-libraries-are-supported)).
- EAS creates only iOS provisioning profiles. A tvOS App Store build needs a tvOS profile created by hand on Apple's developer site, then uploaded to EAS. Projects that also ship iOS should keep tvOS credentials local ([TV guide](https://docs.expo.dev/guides/building-for-tv/)).
- Licenses are permissive: `react-native-tvos` is MIT ([LICENSE](https://github.com/react-native-tvos/react-native-tvos/blob/main/LICENSE)), and so are Expo ([LICENSE](https://github.com/expo/expo/blob/main/LICENSE)) and `expo-video` ([package.json](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/package.json)).

## Video players and HLS

### expo-video

The docs list `android`, `ios`, `web` and `tvos` as platforms ([expo-video docs](https://docs.expo.dev/versions/latest/sdk/video/)). The tvOS podspec target is 16.4 ([podspec](https://github.com/expo/expo/tree/sdk-58/packages/expo-video/ios)).

- **tvOS.** The view is an `AVPlayerViewController`. When fullscreen starts on tvOS, the module presents the controller instead of rotating it, and it skips the orientation, Picture in Picture and timecode code with `#if !os(tvOS)` guards ([VideoView.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoView.swift), [VideoModule.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoModule.swift)). `nativeControls` maps to the tvOS skip forward and back buttons ([VideoModule.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoModule.swift)).
- **Android TV.** The player is Media3 1.9.1 with `media3-exoplayer-hls`, `media3-exoplayer-dash` and `media3-ui` ([build.gradle](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/android/build.gradle)). The module has no TV-specific code. Remote control of the built-in controls comes from Media3's `PlayerView` (search of [the Android sources](https://github.com/expo/expo/tree/sdk-58/packages/expo-video/android/src/main/java/expo/modules/video)).
- **HLS.** `VideoSource.contentType` accepts `'hls'`. DASH and SmoothStreaming are Android-only. The cache does not work with HLS on iOS ([expo-video docs](https://docs.expo.dev/versions/latest/sdk/video/)). The docs do not say whether the tvOS cache behaves like iOS.
- **Tracks.** `audioTrack`, `availableAudioTracks`, `subtitleTrack`, `availableSubtitleTracks`, `videoTrack` and `maxResolution` are documented for "Android, iOS", without tvOS ([expo-video docs](https://docs.expo.dev/versions/latest/sdk/video/)). The Swift code for these properties has no tvOS guard ([VideoModule.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoModule.swift), [VideoPlayerAudioTracks.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoPlayerAudioTracks.swift), [VideoPlayerSubtitles.swift](https://github.com/expo/expo/blob/sdk-58/packages/expo-video/ios/VideoPlayerSubtitles.swift)), so it compiles for tvOS. Whether it behaves correctly there is untested here. On iOS, video tracks only show up for HLS when the URL ends in `.m3u8` or `contentType` is `'hls'` ([expo-video docs](https://docs.expo.dev/versions/latest/sdk/video/)).
- **Not on TV.** Picture in Picture is documented for Android, iOS and web. Display frame rate matching (`VideoChangeFrameRateStrategy`) is Android-only ([expo-video docs](https://docs.expo.dev/versions/latest/sdk/video/)).

### react-native-video

- v6 (6.19.3) declares `:tvos => "13.0"` in its podspec and pulls in the Google IMA tvOS SDK for ads ([podspec](https://unpkg.com/react-native-video@6.19.3/react-native-video.podspec)).
- The v7 README says v7 is in active development with breaking changes and lists "TV Support" as TODO ([README](https://github.com/TheWidlarzGroup/react-native-video/blob/master/README.md)). The v7 beta podspec declares iOS only ([podspec](https://unpkg.com/react-native-video@7.0.0-beta.12/ReactNativeVideo.podspec)). The tracking issue [#4607](https://github.com/TheWidlarzGroup/react-native-video/issues/4607) was closed in 2025, but the README still says TODO.
- The README describes v6 as maintained by the community and TWG, and v7 as the active line ([README](https://github.com/TheWidlarzGroup/react-native-video/blob/master/README.md)). Picking v6 for TV means picking the line that is winding down.

### What each player accepts

This matters for the server's transcoding design.

- **Apple TV (HLS).** Apple's authoring spec allows H.264, HEVC, Dolby Vision and AV1 video. H.264 goes in fMP4 or MPEG-TS segments. HEVC and AV1 must be fMP4. Multichannel audio can be AAC, AC-3, E-AC-3, FLAC or Apple Lossless. Subtitles must be WebVTT or IMSC1 (text profile), and captions can be CEA-608/708 ([HLS authoring spec](https://developer.apple.com/documentation/http-live-streaming/hls-authoring-specification-for-apple-devices)). The list has no image subtitle formats such as PGS. Those would need burning in or OCR on the server.
- **Google TV (Media3).** For HLS, Media3 supports MPEG-TS and fMP4/CMAF segments, with WebVTT and CEA-608/708 text ([Media3 HLS](https://developer.android.com/media/media3/exoplayer/hls)). Outside HLS it plays progressive MP4, Matroska, WebM and MPEG-TS, and loads standalone SubRip, SSA/ASS, TTML and WebVTT files ([Media3 supported formats](https://developer.android.com/media/media3/exoplayer/supported-formats)). Whether a codec decodes depends on the device's decoders, which that page also says.
- So fMP4 HLS with WebVTT plays on both. Google TV could also direct-play many MKV files that Apple TV cannot.

## Focus navigation

What `react-native-tvos` provides ([fork README](https://github.com/react-native-tvos/react-native-tvos#code-changes)):

- `Pressable`, `TouchableHighlight` and `TouchableOpacity` fire `onFocus`, `onBlur`, `onPress` and `onLongPress` from the remote. Focus and blur are native events that bubble through `View`. Nativewind's `focus:` and `active:` classes work on top of these.
- `TVFocusGuideView` wraps Apple's `UIFocusGuide` and has an Android version. Props: `destinations`, `autoFocus` (remembers the last focused child), `trapFocusUp/Down/Left/Right`, and `onFocusEnter`/`onFocusLeave`. The last two were added in 0.86.3-0 and 0.87.1-0 ([release notes](https://github.com/react-native-tvos/react-native-tvos/releases/tag/v0.86.3-0)).
- `nextFocusUp/Down/Left/Right` on `View` now work on tvOS as well as Android.
- `useTVEventHandler` and `TVEventHandler` read remote keys. `BackHandler` covers the Apple TV menu button. `TVEventControl` can turn the menu key and touch-surface pan gestures on or off.
- `FlatList` and other virtualized lists wrap their content in a focus trap. They also take `additionalRenderRegions` so focused areas are never virtualized away.
- `ScrollView` has TV-only props for per-item snapping (`snapToAlignment="item"`, `scrollSnapAlign`, `scrollSnapOffset`) and for scroll animation.
- `Platform.isTV` and `Platform.isTVOS` detect TV at runtime. An optional Metro config resolves `*.tv.tsx`, `*.ios.tv.tsx` and `*.android.tv.tsx` files.
- On Apple TV, native code applies Apple's parallax focus animation. View props can adjust or turn it off.

JavaScript spatial navigation libraries:

- [react-tv-space-navigation](https://github.com/bamlab/react-tv-space-navigation) (BAM, MIT). Its README says to use the native `react-native-tvos` focus engine if you do not need web TV support. It also lists two open problems: React 19 with Expo is "still work-in-progress", and virtualizing hundreds of rows is buggy ([README](https://github.com/bamlab/react-tv-space-navigation/blob/main/README.md)). The last stable release, 5.2.0, and 6.0.0-beta1 are from July 2025 ([npm](https://registry.npmjs.org/react-tv-space-navigation)). The repository's last push was on 2025-10-24 (GitHub API, checked 2026-10-29).
- [Norigin Spatial Navigation](https://github.com/NoriginMedia/Norigin-Spatial-Navigation) (MIT). Its README lists Android TV and Apple TV through React Native as supported. Its [React Native TV guide](https://devportal.noriginmedia.com/docs/Norigin-Spatial-Navigation/guides/react-native-tv) uses the same `useFocusable` hooks as the web package, with `ReactNativeLayoutAdapter` from `@noriginmedia/norigin-spatial-navigation-react-native-tvos` (4.0.0, 2026-07-01, [npm](https://registry.npmjs.org/@noriginmedia%2Fnorigin-spatial-navigation-react-native-tvos)). The README names a different package, `@noriginmedia/norigin-spatial-navigation-react-native`, whose latest version (3.1.5) pins `react-native` 0.84.0 as a peer ([npm](https://registry.npmjs.org/@noriginmedia%2Fnorigin-spatial-navigation-react-native)). Treat that naming mismatch as a sign the React Native support is new.

## Telemetry

- **OpenTelemetry JS.** The README's supported runtimes table has a "React Native, Expo" row. It says those apps use the browser implementations and need Metro 0.81.3 or later, which means React Native 0.79 or later ([opentelemetry-js README](https://github.com/open-telemetry/opentelemetry-js#react-native-support)). The same README calls browser instrumentation experimental. Current versions are `@opentelemetry/api` 1.10.0, `@opentelemetry/sdk-trace-web` 2.12.0 and `@opentelemetry/exporter-trace-otlp-http` 0.300.0, all Apache-2.0 ([npm](https://registry.npmjs.org/@opentelemetry%2Fapi)). These packages have no native code, so the TV fork changes nothing for them. They cannot see native crashes or player internals unless the app reports them from `expo-video` events. The semantic conventions registry has attributes for `session.id` ([session](https://opentelemetry.io/docs/specs/semconv/registry/attributes/session/)), `device.*` ([device](https://opentelemetry.io/docs/specs/semconv/registry/attributes/device/)) and `app.screen.name` ([app](https://opentelemetry.io/docs/specs/semconv/registry/attributes/app/)).
- **Sentry** `@sentry/react-native` 8.32.0. Its `RNSentry.podspec` sets `s.tvos.deployment_target = "11.0"` ([podspec](https://unpkg.com/@sentry/react-native@8.32.0/RNSentry.podspec)). Expo SDK 58 pins `~8.28.0` ([bundledNativeModules.json](https://unpkg.com/expo@58.0.21/bundledNativeModules.json)). Sentry's React Native docs do not mention tvOS or Android TV ([docs](https://docs.sentry.io/platforms/react-native/)). It reports to a Sentry server, not to an OTLP collector.
- **Datadog** `@datadog/mobile-react-native` 3.11.0 declares iOS and tvOS 12.0 ([podspec](https://unpkg.com/@datadog/mobile-react-native@3.11.0/DatadogSDKReactNative.podspec)). Datadog's docs have separate "Android and Android TV" and "iOS and tvOS" SDK pages ([docs](https://docs.datadoghq.com/real_user_monitoring/application_monitoring/react_native/)). Datadog is a hosted service, which does not fit a self-hosted homelab.
- **Embrace** `@embrace-io/react-native` 7.2.0 declares iOS only ([podspec](https://unpkg.com/@embrace-io/react-native@7.2.0/RNEmbraceCore.podspec)).
- **Grafana Faro** `@grafana/faro-react-native` 1.5.0 (Apache-2.0) declares iOS 13.4 only ([podspec](https://unpkg.com/@grafana/faro-react-native@1.5.0/FaroReactNative.podspec)). It depends on MMKV, `react-native-device-info` and PLCrashReporter, and its tracing add-on uses OpenTelemetry ([README](https://unpkg.com/@grafana/faro-react-native@1.5.0/README.md)). Using it on tvOS would mean patching the podspec and checking each native dependency.

## Installing on household TVs

### Apple TV

- **Free Apple account.** Xcode's Personal Team can install on up to 3 devices. App IDs, devices and provisioning profiles expire after 7 days, and the app must be rebuilt and reinstalled after that ([developer account overview](https://developer.apple.com/help/account/basics/about-your-developer-account/)). That rules it out for daily household use.
- **Apple Developer Program.** It costs $99 a year and is required for TestFlight and distribution ([programs](https://developer.apple.com/programs/)).
- **Ad hoc.** Members can register up to 100 Apple TVs per membership year and install signed builds directly on them ([devices overview](https://developer.apple.com/help/account/devices/devices-overview/)). The Apple Distribution certificate covers tvOS builds for designated test devices ([certificates](https://developer.apple.com/help/account/certificates/certificates-overview)).
- **TestFlight.** TestFlight supports tvOS. A build can be tested for 90 days. tvOS testers send feedback by email, not from the app ([TestFlight overview](https://developer.apple.com/help/app-store-connect/test-a-beta-version/testflight-overview/)). Internal testers are up to 100 team members who hold an App Store Connect role. External testers can number up to 10,000, but the first build has to pass App Review for TestFlight ([TestFlight](https://developer.apple.com/testflight/)). For a household, either each person joins the team with a role, or the builds go through beta review. Someone has to upload a new build at least every 90 days.
- **Unlisted App Store app.** The app does not appear in search or charts and opens only from a direct link. It still has to be submitted to App Review ([unlisted app distribution](https://developer.apple.com/support/unlisted-app-distribution/)). Review needs full access to the app, including a demo account or demo mode, and backend services that are live during review ([App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)). For a self-hosted server that means exposing a demo server to Apple.
- EAS needs a hand-made tvOS provisioning profile, as covered above ([TV guide](https://docs.expo.dev/guides/building-for-tv/)).

### Google TV

- **adb.** Turn on Developer options by tapping the build number, then turn on USB or wireless debugging ([developer options](https://developer.android.com/studio/debug/dev-options)). That page does not name the Android TV menu path. Google's verification FAQ says `adb` installs never need developer verification ([verification FAQ](https://developer.android.com/developer-verification/guides/faq)).
- **APK file.** Users can install an APK from a website or file after they allow "install unknown apps" for the app that opens it ([alternative distribution](https://developer.android.com/distribute/marketing-tools/alternative-distribution)). An EAS profile with `EXPO_TV: "1"` and `buildType: "apk"` produces such a file ([TV guide](https://docs.expo.dev/guides/building-for-tv/)).
- **Android developer verification.** From 2026-09-30, certified devices in Brazil, Indonesia, Singapore and Thailand block apps from unverified developers. Google plans to expand this globally in 2027 ([developer verification](https://developer.android.com/developer-verification)). Outside Google Play, enforcement applies only to phones and tablets in those regions for now ([verification FAQ](https://developer.android.com/developer-verification/guides/faq), updated 2026-07-15). A free "limited distribution" account can share an app with up to 20 devices without ID or a fee ([verification guides](https://developer.android.com/developer-verification/guides)). Google has not said when or whether TVs will be enforced.
- **Google Play.** The developer opts in to Android TV in the Play Console. Google reviews the app against the TV quality guidelines. From 2026-08-01, TV apps must support 64-bit and 16 KB page sizes ([distribute to Android TV](https://developer.android.com/training/tv/publishing/distribute)).

### Network access to the homelab server

- Android 9 (API 28) and later block cleartext HTTP by default. Apps must opt in through a network security config ([network security config](https://developer.android.com/privacy-and-security/security-config)). Expo SDK 58 targets API 36 ([SDK table](https://docs.expo.dev/versions/latest/#support-for-android-and-ios-versions)).
- Apple's `NSAllowsLocalNetworking` page says iOS 17 and later block connections to bare IP addresses by default. Its platform list covers iOS, iPadOS, macOS and visionOS, not tvOS ([NSAllowsLocalNetworking](https://developer.apple.com/documentation/bundleresources/information-property-list/nsapptransportsecurity/nsallowslocalnetworking)). The safe choice is HTTPS with a real hostname and certificate for the server.

## Recommendations

These are suggestions for the later grilling ticket, not decisions.

1. Keep Expo with `react-native-tvos` as the TV candidate. Start the prototype on SDK 58 with `react-native-tvos@0.88-stable`. If that fails, fall back to SDK 57 with `0.86-stable`, the pair Expo's own TV examples use.
2. Use `expo-video` as the player. Prototype track switching on tvOS first, because the docs leave it out.
3. Have the server offer fMP4 HLS with H.264 or HEVC video, AAC or AC-3/E-AC-3 audio, and WebVTT subtitles as the format both TVs play. Treat direct play on Google TV as a later optimization.
4. Use the native focus engine (`TVFocusGuideView`, `nextFocus*`, `useTVEventHandler`). Skip the JavaScript spatial navigation libraries unless a web TV target appears.
5. Send app traces and logs with the OpenTelemetry JS web SDK over OTLP/HTTP to the homelab collector. Decide native crash reporting separately.
6. Install on Apple TV with TestFlight internal testing or ad hoc. Install on Google TV with an `adb`-installed or sideloaded APK.

## Open questions

- Which SDK and TV fork pair to start on, given that the compatibility table and examples lag SDK 58.
- Do `expo-video` audio and subtitle track APIs work on tvOS? A prototype on a real Apple TV has to answer this.
- Native player controls or custom JavaScript controls? The native controls differ per platform. tvOS shows its transport bar, and Android shows Media3's `PlayerView` driven by the D-pad.
- Should the server always deliver HLS, or let Google TV direct-play? This decides how much Quick Sync transcoding work the server needs.
- How to handle image subtitles (PGS, VobSub) for Apple TV: burn in, OCR to WebVTT, or skip.
- Is native crash reporting needed? If yes, which backend: a Sentry server, or nothing beyond OTel JS?
- Apple distribution: TestFlight (90-day rebuilds, each person in App Store Connect), ad hoc (registered Apple TVs), or an unlisted App Store app (needs a demo server for App Review). Each needs the $99 yearly membership.
- Will Android developer verification reach Google TV in 2027, and would a limited distribution account (20 devices) cover the household?
- Should the TV app also ship to phones (shared package name), or be TV-only with `androidTVRequired`?
