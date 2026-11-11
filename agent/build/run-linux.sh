#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
case "${1:-}" in
  bridge)
    if [[ "${RELIVO_ALLOW_BRIDGE_GENERATION:-}" != "owner-authorized" ]]; then
      printf '%s\n' 'Owner authorization for bridge generation is required.' >&2
      exit 2
    fi
    command_script=bridge-command.sh
    ;;
  compile) command_script=linux-command.sh ;;
  *) printf 'Usage: %s bridge|compile\n' "$0" >&2; exit 2 ;;
esac
mkdir -p "$workspace/.local/agent-build/cargo" "$workspace/.local/agent-build/pub"
mkdir -p "$workspace/.local/toolchain/cargo/registry" "$workspace/.local/toolchain/cargo/git"
exec docker run --rm --name "relivo-agent-${1}" \
  -e CI=true \
  -e "RELIVO_ALLOW_BRIDGE_GENERATION=${RELIVO_ALLOW_BRIDGE_GENERATION:-}" \
  --mount "type=bind,source=$workspace/rustdesk,target=/work" \
  --mount "type=bind,source=$workspace/agent/build,target=/build-scripts,readonly" \
  --mount "type=bind,source=$workspace/.local/agent-build/cargo,target=/home/builder/.cargo" \
  --mount "type=bind,source=$workspace/.local/toolchain/cargo/registry,target=/home/builder/.cargo/registry" \
  --mount "type=bind,source=$workspace/.local/toolchain/cargo/git,target=/home/builder/.cargo/git" \
  --mount "type=bind,source=$workspace/.local/agent-build/pub,target=/home/builder/.pub-cache" \
  relivo/agent-linux-builder:2026-11-11 bash "/build-scripts/$command_script"
