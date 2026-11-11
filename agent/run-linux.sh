#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
profile="$workspace/.local/relivo-agent"
bundle="$profile/current"
if [[ ! -x "$bundle/rustdesk" || ! -s "$bundle/lib/librustdesk.so" ]]; then
  printf '%s\n' 'The modified Relivo Agent Linux release has not been installed.' >&2
  exit 2
fi
if [[ ! -f "$profile/config/rustdesk/RustDesk2.toml" ]]; then
  printf '%s\n' 'Create the isolated managed profile with agent/configure.py first.' >&2
  exit 2
fi
umask 077
mkdir -p "$profile/cache" "$profile/data"
export XDG_CONFIG_HOME="$profile/config"
export XDG_CACHE_HOME="$profile/cache"
export XDG_DATA_HOME="$profile/data"
export LD_LIBRARY_PATH="$bundle/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$bundle/rustdesk" "$@"
