#!/usr/bin/env bash
# Requires the explicit owner override recorded in docs/agent-delivery.md.
set -euo pipefail
if [[ "${RELIVO_ALLOW_BRIDGE_GENERATION:-}" != "owner-authorized" ]]; then
  printf '%s\n' 'Bridge generation is blocked by rustdesk/AGENTS.md. Obtain the explicit owner override first.' >&2
  exit 2
fi
test -f src/flutter_ffi.rs
test -f flutter/pubspec.yaml
cargo install cargo-expand --version 1.0.95 --locked
cargo install flutter_rust_bridge_codegen --version 1.80.1 --features uuid --locked
(
  cd flutter
  flutter pub get
)
flutter_rust_bridge_codegen \
  --rust-input ./src/flutter_ffi.rs \
  --dart-output ./flutter/lib/generated_bridge.dart \
  --c-output ./flutter/macos/Runner/bridge_generated.h
cp ./flutter/macos/Runner/bridge_generated.h ./flutter/ios/Runner/bridge_generated.h
