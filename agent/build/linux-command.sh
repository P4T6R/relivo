#!/usr/bin/env bash
set -euo pipefail
for generated_file in src/bridge_generated.rs src/bridge_generated.io.rs \
  flutter/lib/generated_bridge.dart flutter/lib/generated_bridge.freezed.dart; do
  if [[ ! -s "$generated_file" ]]; then
    printf 'Missing generated source: %s\n' "$generated_file" >&2
    exit 2
  fi
done

cargo build --locked --lib --release --features flutter,linux-pkg-config

# Match build.py's Linux FFI compatibility adjustment after bridge generation.
python3 - <<'PY'
from pathlib import Path
bridge = Path('flutter/lib/generated_bridge.dart')
text = bridge.read_text()
text = text.replace('ffi.NativeFunction<ffi.Bool Function(DartPort',
                    'ffi.NativeFunction<ffi.Uint8 Function(DartPort')
bridge.write_text(text)
PY

(
  cd flutter
  flutter build linux --release
)
bundle=flutter/build/linux/x64/release/bundle
test -x "$bundle/rustdesk"
test -s "$bundle/lib/librustdesk.so"
test -s "$bundle/lib/libapp.so"
bash /build-scripts/bundle-linux-runtime.sh "$bundle"
sha256sum "$bundle/rustdesk" "$bundle/lib/librustdesk.so" "$bundle/lib/libapp.so"
