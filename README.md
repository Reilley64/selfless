<img src="assets/icon.svg" width="64" height="64" alt="">

# selfless

Created in [T3 Code](https://t3.codes).

## Development

Install [mise](https://mise.jdx.dev) and Docker, then run `mise install` and `bun install` in the repository root.

- `mise run services` starts PostgreSQL and grafana/otel-lgtm from `compose.yaml`. Grafana runs at http://localhost:3000.
- `cargo run` starts the server. It reads `DATABASE__URL` (set in `mise.toml`) and `HTTP__PORT` (default 8080). It serves the web export from `WEB__DIR` (default `presentation/web/dist`) at `/`. It sends traces, metrics and logs over OTLP gRPC to `localhost:4317`. The standard `OTEL_*` variables change this, and `mise.toml` sets `OTEL_SERVICE_NAME`.
- The server relays app telemetry. It takes OTLP/HTTP protobuf at `/api/telemetry/traces/v1`, `/api/telemetry/metrics/v1` and `/api/telemetry/logs/v1` and forwards it over gRPC to the same OTLP endpoint its own exporters use. It checks no token yet.
- The web app reports as `selfless-web` and sends its traces to the relay. Each API call gets a client span and a `traceparent` header, so a trace in Grafana starts in the web app and continues on the server.
- `mise run dev` starts PostgreSQL, the server and the web app's dev server on port 8081. The dev server sends `/actuator` and `/api` requests to the server.
- The TV app in `presentation/tv` reports as `selfless-tv`. It reads the server URL from `EXPO_PUBLIC_SERVER_URL` when Metro bundles it, for example `http://localhost:8080` on the tvOS Simulator and `http://10.0.2.2:8080` on the Android emulator. Its native folders are not committed. `bun run prebuild` in `presentation/tv` writes them for TV with `EXPO_TV=1`. Then `bun run ios` builds a development build for the tvOS Simulator, and `bun run android` builds one for an Android TV emulator. `./gradlew assembleRelease` in `presentation/tv/android` writes a TV-only APK.
- `bun run generate` writes the API client from `openapi.yaml` and the TypeScript telemetry constants from `telemetry/registry`. It also builds the theme from the design tokens in `tokens`. Style Dictionary writes a Tailwind `@theme` file and a native-only font block to `presentation/ui/theme`. Run it after changing any of these. `bun run typecheck`, `bun run lint` and `bun run test` check the apps, and `bun run export` writes the web export to `presentation/web/dist`.
- The server build reads the branch, commit and commit time for `/actuator/info/v1` from git. `GIT_BRANCH`, `GIT_COMMIT` and `GIT_COMMIT_TIME` override them, for builds outside a git checkout.
- A cargo build runs Weaver to generate Rust constants from `telemetry/registry`. `mise run lint:telemetry` checks the registry.
- `mise run test` runs the server tests against that PostgreSQL. Each test creates its own database.
- `mise run check` checks formatting, Clippy lints and crate direction.
- `mise run lint:openapi` lints `openapi.yaml`.
- `mise run build:image` builds the container image for linux/amd64 as `selfless:local`, with the git values and the version as build arguments. `mise run test:image` also starts it beside PostgreSQL and waits for readiness to return `UP`. CI runs the same check on pull requests. On an arm64 Mac, Docker emulates amd64, and the first build takes about 12 minutes.
- The image runs as user 10001. It serves the web export from `/app/web`, and has jellyfin-ffmpeg in `/usr/lib/jellyfin-ffmpeg`. Its license is in `/usr/share/doc/jellyfin-ffmpeg8/copyright`, and `SOURCE` beside it links to the source. The image has no `HEALTHCHECK`. Use the actuator endpoints instead.
- `cargo run --bin toasty -- migration generate` writes a migration for changed models to `infrastructure/persistence/toasty`. The server applies pending migrations when it starts.

## Releases

Every push to `main` runs "Build, release and publish: Application". It builds the image, smoke tests it and pushes `ghcr.io/reilley64/selfless:<sha8>` and `:latest`. release-please keeps a release pull request open. It reads the conventional commit titles on `main` and sets one version for every crate in `Cargo.toml` and `Cargo.lock` and for `expo.version` in `presentation/web/app.json` and `presentation/tv/app.json`. Merging that pull request creates the `vX.Y.Z` release, and the workflow adds the `vX.Y.Z` and `release` tags to the image it built for that commit. Each crate sets `version` itself, because release-please cannot update `version.workspace = true`.

## License

selfless is licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
