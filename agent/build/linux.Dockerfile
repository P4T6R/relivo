# Build environment only. The finished desktop application runs on the host.
FROM rust:1.99.1-bookworm@sha256:e6f2cff35d0f309acf8433b4f05878ed77092602c5065cc81801177da0bb3a9c AS native-deps
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates clang libclang-dev cmake ninja-build nasm pkg-config \
    python3 git curl unzip xz-utils patchelf \
    libgtk-3-dev libayatana-appindicator3-dev libasound2-dev libpulse-dev \
    libxcb-randr0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxdo-dev \
    libxfixes-dev libxtst-dev libxrandr-dev libxinerama-dev libxi-dev \
    libssl-dev libudev-dev libsodium-dev libdbus-1-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
    libvpx-dev libaom-dev libyuv-dev libopus-dev libjpeg-dev \
    libwayland-dev wayland-protocols libegl1-mesa-dev \
    && rm -rf /var/lib/apt/lists/*
ENV CARGO_BUILD_JOBS=1 CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 SODIUM_USE_PKG_CONFIG=1 \
    CMAKE_BUILD_PARALLEL_LEVEL=1

FROM native-deps AS flutter-sdk
ARG FLUTTER_VERSION=3.24.5
ARG FLUTTER_SHA256=a7c82f551a9eae018e078f6bb186171e5a77920d35a3d75a61d9a593d0a9e4ae
RUN curl --fail --location --retry 3 --connect-timeout 20 \
      "https://storage.googleapis.com/flutter_infra_release/releases/stable/linux/flutter_linux_${FLUTTER_VERSION}-stable.tar.xz" \
      --output /tmp/flutter.tar.xz \
    && printf '%s  %s\n' "$FLUTTER_SHA256" /tmp/flutter.tar.xz | sha256sum -c - \
    && tar -xJf /tmp/flutter.tar.xz -C /opt \
    && rm /tmp/flutter.tar.xz \
    && git config --system --add safe.directory /opt/flutter
ENV PATH=/opt/flutter/bin:/usr/local/cargo/bin:$PATH
RUN flutter config --no-analytics --enable-linux-desktop \
    && flutter precache --linux
WORKDIR /work

FROM flutter-sdk AS builder
# Debian ships the library and headers without the pkg-config metadata required
# by scrap's supported linux-pkg-config build path.
RUN yuv_version="$(awk '/^#define LIBYUV_VERSION / {print $3}' /usr/include/libyuv/version.h)" \
    && test -n "$yuv_version" \
    && mkdir -p /usr/local/lib/pkgconfig \
    && printf 'Name: libyuv\nDescription: Debian libyuv\nVersion: %s\nLibs: -lyuv\nCflags: -I/usr/include\n' "$yuv_version" \
       > /usr/local/lib/pkgconfig/libyuv.pc \
    && pkg-config --exists libyuv \
    && useradd --create-home --uid 1000 builder \
    && chown -R builder:builder /opt/flutter
RUN rustup component add rustfmt
ENV CARGO_HOME=/home/builder/.cargo PUB_CACHE=/home/builder/.pub-cache
ENV PATH=/home/builder/.cargo/bin:$PATH
USER builder
