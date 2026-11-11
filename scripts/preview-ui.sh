#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
preview_dir="$project_dir/.local/preview"
profile_dir="$preview_dir/home"
user_home="${HOME:?HOME must identify the current user}"
runtime_path="/run/user/$(id -u)"
app_root="$preview_dir/root"
app_path="/app/usr/share/rustdesk/rustdesk"

for dependency in bwrap dbus-run-session python3; do
    command -v "$dependency" >/dev/null || {
        printf 'Missing preview dependency: %s\n' "$dependency" >&2
        exit 1
    }
done

if [[ -z "${DISPLAY:-}" || ! -d /tmp/.X11-unix ]]; then
    printf 'This desktop preview requires a local X11 session.\n' >&2
    exit 1
fi
if [[ ! -x "$app_root/usr/share/rustdesk/rustdesk" ]]; then
    printf 'The verified preview package has not been extracted. See docs/source-provenance.md.\n' >&2
    exit 1
fi

umask 077
mkdir -p "$profile_dir/.config/rustdesk" "$profile_dir/.cache" \
    "$profile_dir/.local/share" "$preview_dir/runtime"
chmod 700 "$profile_dir" "$preview_dir/runtime"

auth_source="${XAUTHORITY:-$user_home/.Xauthority}"
if [[ -f "$auth_source" ]]; then
    cp -- "$auth_source" "$profile_dir/.Xauthority"
fi

cat > "$profile_dir/.config/rustdesk/RustDesk2.toml" <<'CONFIG'
[options]
stop-service = 'Y'
custom-rendezvous-server = '127.0.0.1'
CONFIG

cat > "$profile_dir/.config/rustdesk/RustDesk_local.toml" <<'CONFIG'
[options]
enable-check-update = 'N'
allow-auto-update = 'N'
CONFIG

mounts=(
    --ro-bind /usr /usr
    --ro-bind /bin /bin
    --ro-bind /sbin /sbin
    --ro-bind /lib /lib
    --ro-bind /lib64 /lib64
    --ro-bind /etc /etc
    --ro-bind /sys /sys
    --proc /proc
    --dev /dev
    --tmpfs /tmp
    --ro-bind /tmp/.X11-unix /tmp/.X11-unix
    --bind "$profile_dir" "$user_home"
    --bind "$preview_dir/runtime" "$runtime_path"
    --ro-bind "$app_root" /app
)
if [[ -d /dev/dri ]]; then
    mounts+=(--dev-bind /dev/dri /dev/dri)
fi
for gpu_device in /dev/nvidia*; do
    if [[ -c "$gpu_device" ]]; then
        mounts+=(--dev-bind "$gpu_device" "$gpu_device")
    fi
done

printf 'Opening the prebuilt RustDesk 1.5.3 interface with networking isolated.\n'
exec bwrap \
    --unshare-net --unshare-pid --unshare-ipc --unshare-uts \
    --die-with-parent --new-session \
    "${mounts[@]}" \
    --chdir /app/usr/share/rustdesk \
    --unsetenv DBUS_SESSION_BUS_ADDRESS \
    --unsetenv DBUS_SYSTEM_BUS_ADDRESS \
    --unsetenv SSH_AUTH_SOCK \
    --setenv XAUTHORITY "$user_home/.Xauthority" \
    --setenv XDG_CONFIG_HOME "$user_home/.config" \
    --setenv XDG_CACHE_HOME "$user_home/.cache" \
    --setenv XDG_DATA_HOME "$user_home/.local/share" \
    --setenv XDG_RUNTIME_DIR "$runtime_path" \
    --setenv GDK_BACKEND x11 \
    --setenv LD_LIBRARY_PATH /app/usr/share/rustdesk/lib \
    /usr/bin/dbus-run-session -- "$app_path"
