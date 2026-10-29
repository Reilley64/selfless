ARG JELLYFIN_FFMPEG_VERSION=8.1.3-1

FROM rust:trixie AS chef
WORKDIR /app
ENV MISE_DATA_DIR=/mise MISE_TRUSTED_CONFIG_PATHS=/app PATH=/mise/shims:$PATH
RUN cargo install cargo-chef --version 0.1.78 --locked
COPY rust-toolchain.toml ./
RUN rustup toolchain install

FROM chef AS tools
COPY --from=jdxcode/mise:2026.10.22-debian /usr/local/bin/mise /usr/local/bin/mise
COPY mise.toml ./
RUN --mount=type=secret,id=github_token,env=GITHUB_TOKEN mise install bun github:open-telemetry/weaver

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM tools AS web
COPY --parents package.json bun.lock bunfig.toml patches presentation/*/package.json ./
RUN bun install --frozen-lockfile
COPY . .
RUN bun run generate && bun run export

FROM chef AS build
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --locked --release --recipe-path recipe.json --bin selfless
COPY . .
COPY --from=web /app/presentation/admin/dist presentation/admin/dist
ARG GIT_BRANCH GIT_COMMIT GIT_COMMIT_TIME
RUN --mount=from=tools,source=/mise,target=/mise \
    --mount=from=tools,source=/usr/local/bin/mise,target=/usr/local/bin/mise \
    cargo build --locked --release --bin selfless

FROM scratch AS jellyfin-ffmpeg
ARG JELLYFIN_FFMPEG_VERSION
ADD https://github.com/jellyfin/jellyfin-ffmpeg/releases/download/v${JELLYFIN_FFMPEG_VERSION}/jellyfin-ffmpeg8_${JELLYFIN_FFMPEG_VERSION}-trixie_amd64.deb /jellyfin-ffmpeg.deb
FROM debian:trixie-slim
ARG JELLYFIN_FFMPEG_VERSION
RUN --mount=from=jellyfin-ffmpeg,target=/mnt/jellyfin-ffmpeg \
    apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates /mnt/jellyfin-ffmpeg/jellyfin-ffmpeg.deb \
    && rm -rf /var/lib/apt/lists/* \
    && echo "https://github.com/jellyfin/jellyfin-ffmpeg/tree/v${JELLYFIN_FFMPEG_VERSION}" > /usr/share/doc/jellyfin-ffmpeg8/SOURCE \
    && useradd --system --uid 10001 --user-group --no-create-home --shell /usr/sbin/nologin selfless
WORKDIR /app
COPY --from=build /app/target/release/selfless ./selfless
COPY --from=web /app/presentation/web/dist ./web
ENV WEB__DIR=/app/web OTEL_SERVICE_NAME=selfless PATH=/usr/lib/jellyfin-ffmpeg:$PATH
USER 10001:10001
EXPOSE 8080
ARG GIT_COMMIT VERSION
LABEL org.opencontainers.image.source="https://github.com/Reilley64/selfless" \
      org.opencontainers.image.revision="$GIT_COMMIT" \
      org.opencontainers.image.version="$VERSION" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0"
ENTRYPOINT ["/app/selfless"]
