#!/usr/bin/env bash
set -euo pipefail
workspace_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -x "$workspace_root/.local/toolchain/cargo/bin/rustup" ]]; then
  export CARGO_HOME="$workspace_root/.local/toolchain/cargo"
  export RUSTUP_HOME="$workspace_root/.local/toolchain/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_PROFILE_DEV_DEBUG=0
if [[ -d "$workspace_root/.local/build-deps/root" ]]; then
  # Older native crates split pkg-config flags at spaces. Keep the sysroot
  # reachable through a stable cache path even when the checkout contains spaces.
  deps_cache="${XDG_CACHE_HOME:-$HOME/.cache}/rustdesk-build-deps"
  deps_id="$(printf '%s' "$workspace_root" | sha256sum | cut -c1-16)"
  mkdir -p "$deps_cache"
  if [[ ! -e "$deps_cache/$deps_id" ]]; then
    ln -s "$workspace_root/.local/build-deps/root" "$deps_cache/$deps_id"
  fi
  export PKG_CONFIG_SYSROOT_DIR="$deps_cache/$deps_id"
  export PKG_CONFIG_PATH="$PKG_CONFIG_SYSROOT_DIR/usr/lib/x86_64-linux-gnu/pkgconfig"
  export CPATH="$PKG_CONFIG_SYSROOT_DIR/usr/include/x86_64-linux-gnu${CPATH:+:$CPATH}"
fi
exec cargo "$@"
