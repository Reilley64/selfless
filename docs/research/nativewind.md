# NativeWind v4 or v5 on Apple TV, Google TV and web

Research for [SELFLESS-32](https://app.plane.so/reilley/browse/SELFLESS-32/). Checked on 2026-10-29. Versions and dates below are as of that day. Claims marked "from source" come from reading code, not from running it.

## Summary

On TV alone, the v5 release candidate is not better than v4. Both versions style focus and press the same way. `focus:` listens to `onFocus`/`onBlur` and `active:` listens to `onPressIn`/`onPressOut`, and `react-native-tvos` fires those events from the remote. Neither project tests on a TV. Reading the v5 source turned up two TV problems that v4 does not have:

- The default Metro setup rewrites every `import ... from "react-native"` to a fixed list of exports. That list has no TV components, so `TVFocusGuideView` and `useTVEventHandler` would come back `undefined` (from source).
- The documented `tv:` variant compares `Platform.OS` with `"tv"`. On Apple TV and Google TV, `Platform.OS` is `"ios"` or `"android"`, so the variant never matches (from source).

Both have workarounds, covered below. v5 rc.0 also has open bugs that hit token-driven themes. Opacity modifiers on CSS-variable colors render no color, and `@theme { --font-sans }` is ignored on native.

Outside TV, v5 fits selfless better:

- v5 uses Tailwind CSS 4, the same major version as topcoat's Tailwind feature (it downloads Tailwind 4.3.2 by default). One Style Dictionary build can then write one `@theme` CSS file for the web app, the TV app and the admin app. With v4 (Tailwind 3), the Expo apps need a JavaScript theme object and the admin app needs CSS, so the token build has two outputs.
- v4 only gets patches now. Moving a v4 app to v5 later means rewriting the Tailwind config into CSS.

Recommendation: the user's rule ("use v5 if it is better for TV") does not fire on the TV evidence alone. Still, the token and topcoat argument favors v5. If the grilling ticket accepts the RC risk, the setup would be:

- Start on v5 rc.0 with exact pins.
- Turn off the import rewrite in the TV app.
- Write `--font-*` tokens to `:root`.
- Use the opacity fix from the issue as a patch.
- Run a short spike on the tvOS Simulator and an Android TV emulator before the first UI spec.

Keep v4.2.7 as the fallback. Neither version names Expo SDK 58 (the latest SDK) as tested. Users report both running on it.

## Versions on 2026-10-29

| Package | Version | Date | Source |
| --- | --- | --- | --- |
| `nativewind` (`latest`) | 4.2.7, uses `react-native-css-interop` 0.2.7 and Tailwind 3 | 2026-09-14 | [npm](https://registry.npmjs.org/nativewind), [release](https://github.com/nativewind/nativewind/releases/tag/nativewind%404.2.7) |
| `nativewind` (`rc`) | 5.0.0-rc.0, needs exactly `react-native-css` 3.1.0-rc.0 and `tailwindcss` >4.1.11 | 2026-09-13 | [npm](https://registry.npmjs.org/nativewind), [release](https://github.com/nativewind/nativewind/releases/tag/5.0.0-rc.0) |
| `react-native-css` (`rc`) | 3.1.0-rc.0 (`latest` is 3.0.7) | 2026-09-13 | [npm](https://registry.npmjs.org/react-native-css), [release](https://github.com/nativewind/react-native-css/releases/tag/3.1.0-rc.0) |
| `expo` (`latest`) | 58.0.21, bundles `react-native` 0.88.1, Reanimated 4.7.1, `react-native-web` 0.21 | 2026-10-28 | [npm](https://registry.npmjs.org/expo), [bundledNativeModules.json](https://unpkg.com/expo@58.0.21/bundledNativeModules.json) |
| `react-native-tvos` (`latest`) | 0.88.0-0 | 2026-10-17 | [npm](https://registry.npmjs.org/react-native-tvos) |
| `tailwindcss` (`latest`) | 4.3.3 | 2026-07-16 | [npm](https://registry.npmjs.org/tailwindcss) |
| `topcoat` | 0.11.0, `tailwind` feature downloads Tailwind CLI 4.3.2 by default | 2026-10-24 | [crates.io](https://crates.io/crates/topcoat), [executable.rs](https://github.com/tokio-rs/topcoat/blob/a7073d14668b85806898edc6b533027f7a90d35c/crates/topcoat-tailwind/src/build/executable.rs#L16) |
| `style-dictionary` | 5.7.1 | 2026-10-24 | [npm](https://registry.npmjs.org/style-dictionary), [CHANGELOG](https://github.com/style-dictionary/style-dictionary/blob/3815256076a915e2b864aedb69c87429801b1c31/CHANGELOG.md) |
| DTCG Format, Color and Resolver modules | 2025.10, "Final Community Group Report" | 2025-10-28 | [format](https://www.designtokens.org/TR/2025.10/format/), [color](https://www.designtokens.org/TR/2025.10/color/), [resolver](https://www.designtokens.org/TR/2025.10/resolver/) |

## How each version fits the latest Expo SDK

- **v4.2.7** says it supports "the Expo SDK 57 toolchain" and keeps the Tailwind 3 config, the Babel preset and `withNativeWind(config, { input })` ([release](https://github.com/nativewind/nativewind/releases/tag/nativewind%404.2.7), [installation](https://www.nativewind.dev/docs/getting-started/installation)). Its peer ranges are open (`react-native` `*`, Reanimated `>=3.6.2`) ([npm](https://registry.npmjs.org/react-native-css-interop)). A user runs 4.2.7 in production on Expo SDK 58, React Native 0.88 and Reanimated 4.7.1 ([#1875](https://github.com/nativewind/nativewind/issues/1875)).
- **v5 rc.0** targets "Expo 57.0.22, React Native 0.86.3, React 19.2.3, Reanimated 4.5.1, and Worklets 0.10.1". The release says older SDKs need separate checks ([release](https://github.com/nativewind/nativewind/releases/tag/5.0.0-rc.0)). It does not mention SDK 58. The engine declares `react-native >=0.81` and `@expo/metro-config >=54` ([npm](https://registry.npmjs.org/react-native-css)). A user runs rc.0 on Expo SDK 58 and React Native 0.88 and reports one missing export ([react-native-css #506](https://github.com/nativewind/react-native-css/issues/506)).
- The v5 install pins `tailwindcss@4.1.12`, `@tailwindcss/postcss@4.1.12` and `lightningcss@1.30.1`. It replaces the v4 Babel preset with `withNativewind` in Metro, a `postcss.config.mjs` and a `global.css` with `@import` lines ([v5 installation](https://www.nativewind.dev/v5/getting-started/installation), [migrate from v4](https://www.nativewind.dev/v5/guides/migrate-from-v4)).

## Focus and pressed styles on TV

### What react-native-tvos provides

- `Pressable`, `TouchableHighlight` and `TouchableOpacity` get native TV events. `onFocus`/`onBlur` fire when focus moves. `onPressIn`/`onPressOut` fire when the remote's select button goes down and up. The README says these events "allow RNTV to support the `focus:` and `active:` pseudo classes for Tailwind styles" and links NativeWind's states page. `TouchableNativeFeedback` and `TouchableWithoutFeedback` get no focus events ([fork README](https://github.com/react-native-tvos/react-native-tvos#code-changes)).
- `active:` support came from a fork change in 0.76 that routes TV select events through `Pressability`. That PR notes that Apple TV sends one select event, not a down/up pair, so on Apple TV the pressed state is short ([react-native-tvos #803](https://github.com/react-native-tvos/react-native-tvos/pull/803)). NativeWind first tried a TV fix of its own and reverted it in favor of the fork change ([nativewind #1061](https://github.com/nativewind/nativewind/pull/1061), [#1090](https://github.com/nativewind/nativewind/pull/1090)).
- `Pressable` on the fork renders with `focusable={focusable !== false && isTVSelectable !== false}`, so every `Pressable` is a focus stop unless told otherwise ([Pressable.js](https://github.com/react-native-tvos/react-native-tvos/blob/tvos-v0.88.0/packages/react-native/Libraries/Components/Pressable/Pressable.js#L420)).

### How NativeWind wires the states

Both versions work the same way. These states only work on components that take the matching event props, such as `Pressable` and `TextInput` ([v4 states](https://www.nativewind.dev/docs/core-concepts/states), [v5 states](https://www.nativewind.dev/v5/core-concepts/states)).

| Variant | v4.2.7 | v5 rc.0 |
| --- | --- | --- |
| `focus:` | `onFocus`/`onBlur` handlers ([native-interop.ts](https://github.com/nativewind/nativewind/blob/nativewind%404.2.7/packages/react-native-css-interop/src/runtime/native/native-interop.ts#L223)) | `onFocus`/`onBlur` handlers ([styles/index.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/native/styles/index.ts), [interaction.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/native/react/interaction.ts)) |
| `active:` | `onPressIn`/`onPressOut` | `onPressIn`/`onPressOut` |
| `group-focus:`, `group-active:` | yes | yes |
| `focus-visible:` | no | no; the compiler drops pseudo-classes other than hover, active, focus, disabled and empty ([selectors.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/compiler/selectors.ts#L241-L256)) |
| `tv:` variant | none | documented ([v5 theme](https://www.nativewind.dev/v5/customization/theme)) but never matches on TV, see below |
| Tested on a TV | no | no; no TV code or tests in either repository (search of both repos) |

The v5 RC release audit ran 4,985 checks across compiler, runtime, tooling, rendering and interaction layers. Android checks used an emulator, and the release mentions one check on a physical iPhone ([release](https://github.com/nativewind/nativewind/releases/tag/5.0.0-rc.0)). It names no TV target.

### Known TV problems

**Plain views become focus stops (both versions).** When a `View` carries a hover, active or focus class, or is a `group`/container, both engines swap it for a `Pressable`. v4 does it in [native-interop.ts](https://github.com/nativewind/nativewind/blob/nativewind%404.2.7/packages/react-native-css-interop/src/runtime/native/native-interop.ts#L238) and [render-component.tsx](https://github.com/nativewind/nativewind/blob/nativewind%404.2.7/packages/react-native-css-interop/src/runtime/native/render-component.tsx#L64), and a container turns on all three states ([native-interop.ts](https://github.com/nativewind/nativewind/blob/nativewind%404.2.7/packages/react-native-css-interop/src/runtime/native/native-interop.ts#L844)). v5 does it in [useNativeCss.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/native/react/useNativeCss.ts#L172) and [rules.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/native/react/rules.ts#L116). Because the fork makes `Pressable` focusable, a `<View className="group">` around a poster would itself take focus on TV (from source). An open v5 engine PR stops making containers into `Pressable`s. Its author measured on Android that such a view drops from `clickable="true" focusable="true"` to neither ([react-native-css #482](https://github.com/nativewind/react-native-css/pull/482), open). v4 will not get that change. Until then, put `group` on the focusable `Pressable` itself or pass `focusable={false}`.

**TV exports hidden by the import rewrite (v5 only, from source).** `withNativewind` turns on `globalClassNamePolyfill` by default ([metro.tsx](https://github.com/nativewind/nativewind/blob/7dbaf1829398f1fbca968f054d0079b1837eff46/src/metro.tsx#L11), [withNativewind docs](https://www.nativewind.dev/v5/api/with-nativewind)). On native, the resolver then sends every `import ... from "react-native"` outside `react-native` itself to `react-native-css/components` ([resolver.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/metro/resolver.ts#L36)). That module is a hand-written list of 99 core exports and has no `TVFocusGuideView`, `TVEventHandler`, `useTVEventHandler`, `TVEventControl` or `TVTextScrollView` ([index.cts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/components/index.cts)). The same gap already broke `AssetRegistry` for a user on SDK 58 ([react-native-css #506](https://github.com/nativewind/react-native-css/issues/506)). There are two ways out. One is `withNativewind(config, { globalClassNamePolyfill: false })` in the TV app, then importing styled primitives from `react-native-css/components` or wrapping them with `styled` ([react-native-css README](https://github.com/nativewind/react-native-css#readme)). The other is an upstream PR that adds the TV exports. v4 hooks in through a JSX runtime (`jsxImportSource: "nativewind"` in the Babel config) instead of rewriting imports, so it does not have this problem ([v4 installation](https://www.nativewind.dev/docs/getting-started/installation)).

**`tv:` never matches (v5 only, from source).** `nativewind/theme` defines `@custom-variant tv (@media (display-mode: tv))` ([theme.css](https://github.com/nativewind/nativewind/blob/7dbaf1829398f1fbca968f054d0079b1837eff46/theme.css#L62)). The runtime checks `display-mode` as `value === "native" || Platform.OS === value` ([media-query.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/native/conditions/media-query.ts#L50)). On the fork, `Platform.OS` is `"ios"` on Apple TV and `"android"` on Google TV. TV is a separate `Platform.isTV` flag ([Platform.ios.js](https://github.com/react-native-tvos/react-native-tvos/blob/tvos-v0.88.0/packages/react-native/Libraries/Utilities/Platform.ios.js#L17), [Platform.android.js](https://github.com/react-native-tvos/react-native-tvos/blob/tvos-v0.88.0/packages/react-native/Libraries/Utilities/Platform.android.js#L17)). Since selfless has a separate TV app, its own `global.css` can carry TV values, so `tv:` is not needed.

**Other open bugs.**

- v4 0.2.7: infinite `animate-*` classes (for example `animate-pulse` on a loading skeleton) keep running on the UI thread after unmount. The reporter saw this on Expo SDK 58 with high CPU on iOS ([#1875](https://github.com/nativewind/nativewind/issues/1875), open). This matters on low-power TV hardware.
- v4: the dev-only LogBox renders broken on React Native 0.86+ with the Babel preset ([#1834](https://github.com/nativewind/nativewind/issues/1834), open).
- v5 rc.0: for a color defined as a CSS variable, an opacity modifier such as `bg-primary/50` renders no color on native. The reporter calls this a regression from 3.0.7 ([react-native-css #481](https://github.com/nativewind/react-native-css/issues/481), [nativewind #1870](https://github.com/nativewind/nativewind/issues/1870), both open; the engine's `color-mix.ts` is unchanged on `main` as of 2026-10-21). The issue includes a one-line fix.
- v5 rc.0: `@theme { --font-sans: ... }` has no effect on iOS and Android. Setting it in `:root` works ([#1876](https://github.com/nativewind/nativewind/issues/1876), open).
- v5: a variable defined from another variable, such as `--color-primary: hsl(var(--primary))`, does not update when `VariableContextProvider` changes `--primary` ([react-native-css #213](https://github.com/nativewind/react-native-css/issues/213), open since 2025-10).
- v5 rc.0 and the fork: on Android, cancelling a CSS animation can leave the last transform in place. This is a Reanimated bug ([known issues](https://github.com/nativewind/nativewind/blob/7dbaf1829398f1fbca968f054d0079b1837eff46/docs/known-issues.md), [Reanimated #10507](https://github.com/software-mansion/react-native-reanimated/issues/10507)).

## Performance

No one has published a v4 versus v5 benchmark, and nobody has measured either one on TV hardware.

- In 2023 a benchmark of 1,000 styled views found v4 about 4x slower than `StyleSheet`. NativeWind's creator traced most of the cost to a Reanimated workaround that makes every component animated ([discussion #642](https://github.com/nativewind/nativewind/discussions/642)). In 2025-09 a maintainer (danstepanov) wrote in the same thread that v5 should bring "some performance improvements". He gave no numbers.
- On v5's engine, a Jest measurement on 2026-10-29 found each styled element costs about 2.8x a plain `View` to mount, with or without a `className`, because of the wrapper component and a rule pass ([react-native-css #546](https://github.com/nativewind/react-native-css/issues/546), open). A fix that skips a duplicate style pass on first mount landed on `main` after the RC ([eda2b28](https://github.com/nativewind/react-native-css/commit/eda2b28)).
- An open v5 PR paints `active:` and `group-active:` on the UI thread through a new Reanimated API, so press feedback stops waiting on the JS thread ([react-native-css #484](https://github.com/nativewind/react-native-css/pull/484)). It is Android-tested only and stops turning elements into `Pressable`s. How it treats TV remote presses is unknown.
- Both versions compile classes at build time. On the web, both use a real CSS stylesheet, not runtime styles ([v5 overview](https://www.nativewind.dev/v5)).

If the TV app shows long poster grids, measure mount time on an Android TV emulator in the spike instead of trusting either number.

## Expo web

- Both run on Expo web through Metro and `react-native-web`. On web, both use Tailwind's CSS stylesheet and add `className` support to React Native Web components ([v4 overview](https://www.nativewind.dev/docs), [v5 overview](https://www.nativewind.dev/v5)).
- v5 brings Tailwind 4, which needs Chrome 111, Safari 16.4 or Firefox 128. Those versions came out in 2023 and 2024 ([Tailwind compatibility](https://tailwindcss.com/docs/compatibility)).
- On web, v5's `VariableContextProvider` renders a `<div style="display: contents">` that sets the CSS variables, so one theme provider works on web and native ([web/api.tsx](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/web/api.tsx)).
- v5 dark mode follows `prefers-color-scheme`. `Appearance.setColorScheme()` overrides it on native only. The browser needs its own toggle, and `:root.dark` is rejected on native ([v5 dark mode](https://www.nativewind.dev/v5/core-concepts/dark-mode)).
- The RC lists web limits that do not matter much here. Images need `resizeMode` or `contentFit` on web, and WebKit needs an explicit style for `select-none` ([rc-compatibility.md](https://github.com/nativewind/nativewind/blob/7dbaf1829398f1fbca968f054d0079b1837eff46/docs/rc-compatibility.md)).
- In a monorepo, v5 needs an `@source "../../packages/ui/src"` line so Tailwind sees classes in the shared `ui` package ([v5 monorepos](https://www.nativewind.dev/v5/guides/using-with-monorepos)). In v4 the same paths go in `content` in `tailwind.config.js`.

## Stability of v5

- v5 had five previews from 2025-09-24 to 2026-05-15, then rc.0 on 2026-09-13. Nothing has been published since, apart from nightlies ([npm](https://registry.npmjs.org/nativewind)).
- Stable promotion "follows RC user feedback and verification of the v4 to v5 migration skill against representative applications". The release gives no date ([release](https://github.com/nativewind/nativewind/releases/tag/5.0.0-rc.0)).
- rc.0 needs the exact engine `react-native-css` 3.1.0-rc.0, so the two packages move together ([migrate from v4](https://www.nativewind.dev/v5/guides/migrate-from-v4)).
- The engine repository is busy. Since 2026-09-01 it has 42 new open issues, most of them CSS edge cases filed on 2026-10-29, and several open fixes ([issues](https://github.com/nativewind/react-native-css/issues)). Some of those fixes may change behavior before a stable release.
- v4 still gets patch releases (4.2.2 to 4.2.7 in 2026) ([releases](https://github.com/nativewind/nativewind/releases)). The docs call v4 "the stable release" ([llms.txt](https://www.nativewind.dev/llms.txt)).

## From DTCG token files to Tailwind themes

### The token format and tools

- DTCG 2025.10 is the first stable version. The Format module recommends `.tokens` or `.tokens.json` files and uses `{group.token}` references. Colors are objects with `colorSpace`, `components`, optional `alpha` and optional `hex`. Dimensions are `{ "value", "unit" }` with `px` or `rem` ([format](https://www.designtokens.org/TR/2025.10/format/), [color](https://www.designtokens.org/TR/2025.10/color/)).
- The Resolver module describes themes. A `.resolver.json` file lists `sets` and `modifiers`, for example a `theme` modifier with `light` and `dark` contexts, plus a `resolutionOrder` ([resolver](https://www.designtokens.org/TR/2025.10/resolver/)).
- Style Dictionary 5.7.1 reads DTCG files. It supports the 2025.10 color objects (5.3.0), dimension objects (5.4.0), duration objects and `$root` tokens (5.7.0) ([CHANGELOG](https://github.com/style-dictionary/style-dictionary/blob/3815256076a915e2b864aedb69c87429801b1c31/CHANGELOG.md)). Gradients are in progress and the Resolver module is not supported ([#1590](https://github.com/style-dictionary/style-dictionary/issues/1590), open). Its DTCG page says 2025.10 support is "a work in progress in v5" ([DTCG page](https://styledictionary.com/info/dtcg/)). Selfless would build one Style Dictionary run per theme, each with its own source files, or write a small script that reads the resolver file.
- Style Dictionary has no built-in Tailwind format ([#843](https://github.com/style-dictionary/style-dictionary/issues/843), open). Its official Tailwind example builds a Tailwind 3 preset with Style Dictionary 4 ([tailwind-preset example](https://github.com/style-dictionary/style-dictionary/blob/3815256076a915e2b864aedb69c87429801b1c31/examples/advanced/tailwind-preset/README.md)).
- Terrazzo is another DTCG tool. Its `@terrazzo/plugin-tailwind` 2.10.0 (2026-10-21) generates a Tailwind 4 theme from DTCG tokens ([README](https://unpkg.com/@terrazzo/plugin-tailwind@2.10.0/README.md)). This note did not check its Resolver support.

### With v5 (Tailwind 4 everywhere)

Tailwind 4 keeps the theme in CSS. `@theme { --color-mint-500: ...; }` creates `bg-mint-500` and friends. Variable names must use Tailwind's namespaces (`--color-*`, `--font-*`, `--text-*`, `--spacing-*`, `--radius-*`, `--shadow-*`, `--breakpoint-*` and others). A theme file can live in its own package and be `@import`ed by several projects ([Tailwind theme variables](https://tailwindcss.com/docs/theme)). v5 reads the same `@theme` blocks ([v5 theme](https://www.nativewind.dev/v5/customization/theme)). topcoat's build script takes a custom input CSS file with `@theme` ([topcoat Tailwind docs](https://github.com/tokio-rs/topcoat/blob/a7073d14668b85806898edc6b533027f7a90d35c/docs/tailwind.md)).

A pipeline could look like this:

1. Token files in a `tokens` package, grouped so Style Dictionary's kebab-case names land in Tailwind namespaces. For example, `color.surface.default` becomes `--color-surface-default`.
2. Style Dictionary's built-in `css/variables` format takes a `selector` option and wraps the variables in it ([formats.js](https://github.com/style-dictionary/style-dictionary/blob/3815256076a915e2b864aedb69c87429801b1c31/lib/common/formats.js#L190)). With `selector: "@theme"` it writes a Tailwind theme block. With `selector: ":root"`, `'[data-theme="dark"]'` or `["@media (prefers-color-scheme: dark)", ":root"]` it writes the value blocks (from source, not run).
3. The web app's and TV app's `global.css` import the generated `theme.css` after `nativewind/theme`. The admin app's input CSS imports the same file. topcoat only tracks files inside its own package, so an input outside the package needs its own `cargo:rerun-if-changed` line ([topcoat Tailwind docs](https://github.com/tokio-rs/topcoat/blob/a7073d14668b85806898edc6b533027f7a90d35c/docs/tailwind.md)).
4. The TV app can import a second generated file with TV sizes (larger type and spacing for a TV viewed across the room), so `tv:` is not needed.

Points that matter on native:

- **Units.** NativeWind uses a `rem` of 14 on native and 16 on web, and treats `px` as density-independent points ([v5 units](https://www.nativewind.dev/v5/core-concepts/units), [v4 differences](https://www.nativewind.dev/docs/core-concepts/differences)). Emit dimension tokens in `px` so web, TV and admin agree.
- **Colors.** The v5 compiler converts `oklch`, `lab`, `display-p3` and other spaces to values React Native accepts at build time ([declarations.ts](https://github.com/nativewind/react-native-css/blob/2421ea77da52e5d241d0e4c2cbe50dcddcc7a7be/src/compiler/declarations.ts)), so DTCG color spaces survive. Emitting `hex` is still the simplest choice for matching admin, web and TV.
- **Runtime themes.** The v5 native compiler inlines variable values unless they are listed in `inlineVariables.exclude`. A theme switch then wraps the tree in `VariableContextProvider` with the new values ([v5 dynamic themes](https://www.nativewind.dev/v5/guides/themes)). Style Dictionary can write both the exclude list and one value map per theme (for example with `json/flat`). Because of [react-native-css #213](https://github.com/nativewind/react-native-css/issues/213), the theme layer should hold final values (`outputReferences: false`), not `var()` chains.
- **Fonts.** Until [#1876](https://github.com/nativewind/nativewind/issues/1876) is fixed, write `--font-*` tokens to `:root` as well as `@theme`.
- **Opacity modifiers.** Until [react-native-css #481](https://github.com/nativewind/react-native-css/issues/481) is fixed, `bg-token/50` on a variable color fails on native. Patch it locally with the one-line fix from the issue, or avoid opacity modifiers on themed colors.

### With v4 (Tailwind 3 in the Expo apps)

- The theme lives in `tailwind.config.js`. Dynamic themes use colors like `rgb(var(--color-values) / <alpha-value>)`, values set in `:root` by a plugin, and `vars()` on a wrapping `View` to switch them ([v4 themes](https://www.nativewind.dev/docs/guides/themes)). Style Dictionary's Tailwind example writes this shape: a plugin with `:root` variables plus a JS color map that points at them ([tailwind-preset example](https://github.com/style-dictionary/style-dictionary/blob/3815256076a915e2b864aedb69c87429801b1c31/examples/advanced/tailwind-preset/README.md)).
- topcoat runs Tailwind 4, so the admin app needs a second output: either the CSS `@theme` file above, or the v3 JS config loaded with Tailwind 4's `@config` directive. Tailwind documents `@config` for loading "a legacy JavaScript-based configuration file" ([Tailwind directives](https://tailwindcss.com/docs/functions-and-directives)). This note did not check whether topcoat's standalone CLI accepts `@config` with a JS file.
- The token build then has two formats and two Tailwind majors.

## Open questions for the decision

1. Does the user's rule mean "use v5 only if it beats v4 on TV", or "use v5 unless it is worse on TV"? On TV the two are even in design, and v5 has two TV problems with workarounds. The case for v5 rests on the shared Tailwind 4 theme and on avoiding a later migration.
2. A spike on the tvOS Simulator and an Android TV emulator with Expo 58, `react-native-tvos` 0.88 and v5 rc.0 should check five things:
   - `focus:` and `active:` styles on `Pressable`.
   - Whether a `group` view takes focus.
   - Whether `TVFocusGuideView` imports as `undefined`.
   - Whether the TV app works with `globalClassNamePolyfill: false`.
   - Mount time for a long poster row.
   It would also check whether rc.0 needs anything beyond the [react-native-css #506](https://github.com/nativewind/react-native-css/issues/506) fix on SDK 58.
3. If the TV app turns off the import rewrite, should the `ui` package import primitives from `react-native-css/components` in both apps, so both apps share one import style?
4. Should the Expo apps and the admin app pin the same Tailwind version? The v5 RC pins 4.1.12, and topcoat downloads 4.3.2 unless told `.version("4.1.12")` ([topcoat Tailwind docs](https://github.com/tokio-rs/topcoat/blob/a7073d14668b85806898edc6b533027f7a90d35c/docs/tailwind.md)).
5. Which tokens change at runtime: only light/dark, or per-user themes? This sets the `inlineVariables.exclude` list and the outputs per theme.
6. Should themes use the DTCG Resolver module with a small custom script, or plain token files per theme until Style Dictionary supports resolvers?
7. Should someone open upstream PRs for the TV exports and the `tv:` variant? Both are small, and fixing them would make v5 the clear choice for TV.
8. Expo's Tailwind guide names Uniwind next to NativeWind for universal Tailwind ([Expo Tailwind guide](https://docs.expo.dev/guides/tailwind/)). This note did not evaluate it.
