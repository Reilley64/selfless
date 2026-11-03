# Device checks

A device check runs the TV app on the Android TV emulator or the tvOS Simulator, or the web app in a browser, against a server of your own. Other agents run checks at the same time, so every device, database and port you use is yours alone until you clean up.

## 1. Take a device

- Use a device that is already installed. List them with `~/Library/Android/sdk/emulator/emulator -list-avds` (the Android TV one is `selfless-google-tv`) and `xcrun simctl list devices available`. Never create or clone one.
- Lock the device before you boot it, install on it or drive it: `mkdir ~/.device-locks/<udid or AVD name>`, then write your ticket and purpose into an `owner` file inside it. `mkdir` fails while another agent holds the lock. Then use another device of the same platform, or wait. A lock whose `owner` file is older than 2 hours is stale, and you may take it.

## 2. Start a server

1. Create a scratch database (`docs/agents/migrations.md`, "Scratch databases").
2. Pick a free port, one for which `lsof -iTCP:<port> -sTCP:LISTEN` prints nothing.
3. Start the server from your worktree: `mise exec -- env DATABASE__URL=<scratch URL> HTTP__PORT=<port> cargo run`. It is up when `curl localhost:<port>/actuator/health/readiness/v1` reports `UP`. The TV app asks for this server's address on first launch. `README.md` gives the host name each platform uses.
4. Seed users with `mise run dev:seed-users`, against the same database and your server's URL. It prints two users with their passwords, and a second run does not fail.

For a web check, run `bun run export` before you start the server. The server serves the export at `/`.

## 3. Start Metro for the TV app

In `presentation/tv`, run `bunx expo start --dev-client --port <free port>`. Other agents use the default ports.

A class name used for the first time, or a token after `bun run generate:theme`, does nothing until Metro restarts with `--clear`.

## 4. Drive the device

### Android TV emulator

- Drive it with T3 Code's device tools. `device_list` gives the emulator's `hostId` and `id`, and `device_open` returns an `agent-device` command bound to it. Run every launch, key press, tap, text entry and screenshot through that command.
- The dev client `dev.reilley.selfless` is usually installed already. Rebuild it only when it is missing or a native dependency changed:
  1. In `presentation/tv`, run `bun run prebuild`.
  2. In `presentation/tv/android`, run `./gradlew assembleDebug -PreactNativeArchitectures=arm64-v8a` with `JAVA_HOME=/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home` and `ANDROID_HOME=~/Library/Android/sdk`. A newer Java fails with "Unsupported class file major version".
  3. Install the APK with the `agent-device` `install` command.
- Forward Metro's port with `adb reverse tcp:<metro port> tcp:<metro port>`. `adb` is in `~/Library/Android/sdk/platform-tools`.
- Open the dev client on your Metro with the deep link `exp+selfless-tv://expo-development-client/?url=http%3A%2F%2Flocalhost%3A<metro port>`. `open dev.reilley.selfless --metro-port <port>` alone leaves it on the launcher. Back closes the dev menu sheet.
- Clear the app's stored address and users with `adb shell pm clear dev.reilley.selfless`.
- `tv-remote longpress select` sends no real hold, so the app sees a short press. Long-press with a touch instead: `longpress @<ref> 1200`.
- On emulators, agent-device's `open` switches to a test keyboard that draws nothing (`--no-test-ime` keeps the real one). `adb shell dumpsys input_method | grep mInputShown` tells whether Select opened the keyboard.

### tvOS Simulator

- T3 Code's device tools list no tvOS simulator. Drive it with agent-device directly: `node ~/.t3/tools/agent-device/<version>/node_modules/agent-device/bin/agent-device.mjs --platform ios --target tv --udid <udid> --session <name> <command>`. The commands include `open`, `tv-remote press select|menu|up|down|left|right`, `type` and `screenshot`.
- If a built dev client exists at `~/Library/Developer/Xcode/DerivedData/selfless-*/Build/Products/Debug-appletvsimulator/selfless.app`, install it with `xcrun simctl install <udid> <path>`. If not, run `bun run prebuild` and then `bun run ios` in `presentation/tv`.
- `open dev.reilley.selfless --metro-port <port>` connects the dev client to your Metro. If it stays on the launcher, open the deep link from the Android TV section with `open '<deep link>'`.
- Clear the app's stored address and users with `xcrun simctl keychain <udid> reset`.
- Finish each text field with the keyboard's "done" or "next". Menu cancels the edit, and the field goes back to its old text.

### Web app in a browser

- Use T3 Code's browser preview on `http://localhost:<server port>`.
- To check behaviour on an origin that is not a secure context, such as the homelab's `http://selfless.localdomain`, open `http://<LAN IP>:<server port>`. There `crypto.randomUUID` is undefined.
- Without the T3 browser, use Playwright from a temporary directory outside the repository: `PATH=$(mise where -C <repo> bun)/bin:$PATH bun add playwright-core`, with Chromium from `~/Library/Caches/ms-playwright/` as `executablePath`. `bun add` inside the repository changes its `package.json`.

## 5. Clean up

Clean up after every check, also a failed one:

1. Stop the server and Metro.
2. Remove the port forward with `adb reverse --remove tcp:<metro port>`.
3. Drop the scratch database.
4. Delete `presentation/tv/android` or `presentation/tv/ios` if you built them. Each holds over 1 GB.
5. Release the lock: `rm -rf ~/.device-locks/<udid or AVD name>`.

## TV behaviour to expect

- tvOS draws its own text field, a pill that turns white and grows when focused. UIKit picks the typed text colour, so the app sets the dark appearance in `presentation/tv/app/_layout.tsx`.
- A form focuses its first field with `useFirstFieldFocus()` from `presentation/tv/components/form.ts`. `FocusGuide autoFocus` alone focuses nothing on a cold start, and `hasTVPreferredFocus` does nothing on Android TV.
- On Android TV, `px-*` does not pad a `TextInput`, but `pl-*` and `pr-*` do. `placeholder:text-<token>` sets the placeholder colour on both TVs.
