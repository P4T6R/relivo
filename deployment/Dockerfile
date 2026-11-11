# syntax=docker/dockerfile:1
FROM node:22.23.2-bookworm-slim@sha256:48e4b67d85f87bd551df43704e24d252f56cc5f8e9718841aace50f19948f0f9 AS web-build
WORKDIR /build/web
COPY management/web/package.json management/web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY management/web/ ./
RUN npm run build

FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS rust-build
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev libx11-dev libudev-dev libsodium-dev sqlite3 \
    && rm -rf /var/lib/apt/lists/*
ENV CARGO_BUILD_JOBS=1 CARGO_PROFILE_RELEASE_LTO=thin SODIUM_USE_PKG_CONFIG=1
WORKDIR /build
COPY management/Cargo.toml management/Cargo.lock ./management/
COPY management/src ./management/src
COPY rustdesk-server ./rustdesk-server
COPY deployment/compile-schema.sql /build/compile-schema.sql
RUN sqlite3 /build/compile.sqlite3 < /build/compile-schema.sql
ENV DATABASE_URL=sqlite:///build/compile.sqlite3
RUN --mount=type=cache,id=rustdesk-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=rustdesk-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=rustdesk-management-target,target=/build/management/target,sharing=locked \
    --mount=type=cache,id=rustdesk-server-target,target=/build/rustdesk-server/target,sharing=locked \
    cargo build --locked --release --manifest-path management/Cargo.toml \
    && cargo build --locked --release --manifest-path rustdesk-server/Cargo.toml --bin hbbs --bin hbbr \
    && mkdir /out \
    && cp management/target/release/rustdesk-management rustdesk-server/target/release/hbbs rustdesk-server/target/release/hbbr /out/

FROM debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl netcat-openbsd libssl3 libx11-6 libudev1 libsodium23 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 control \
    && useradd --uid 10001 --gid 10001 --no-create-home --home-dir /tmp control \
    && install -d -o 10001 -g 10001 -m 0700 /data
COPY rustdesk-server/LICENSE /usr/share/licenses/rustdesk/LICENSE
LABEL org.opencontainers.image.licenses="AGPL-3.0-only"
USER 10001:10001
ENV HOME=/tmp
WORKDIR /data

FROM runtime AS management
COPY --from=rust-build /out/rustdesk-management /usr/local/bin/rustdesk-management
COPY --from=web-build /build/web/dist /opt/control/web
ENV MANAGEMENT_DATABASE=/data/management.sqlite3 MANAGEMENT_WEB_DIR=/opt/control/web
STOPSIGNAL SIGINT
ENTRYPOINT ["/usr/local/bin/rustdesk-management"]

FROM runtime AS server
COPY --from=rust-build /out/hbbs /out/hbbr /usr/local/bin/
CMD ["hbbs"]

FROM caddy:2.11.9-alpine@sha256:0bbb055bda6793c999bde78b1b5ce3e89f829fb031af2cf50056ea1246559fee AS edge
RUN addgroup -g 10001 -S control \
    && adduser -u 10001 -S -D -H -G control control \
    && chown -R 10001:10001 /data /config \
    && chmod 0700 /data /config
COPY deployment/Caddyfile /etc/caddy/Caddyfile
USER 10001:10001
