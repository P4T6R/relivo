#!/usr/bin/env bash
set -euo pipefail
umask 077
workspace_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace_root"
runtime_dir="$workspace_root/.local/management"
mkdir -p "$runtime_dir"
export MANAGEMENT_DATABASE="$runtime_dir/management.sqlite3"
export MANAGEMENT_INTERNAL_KEY_FILE="$runtime_dir/internal-key"
export MANAGEMENT_WEB_DIR="$workspace_root/management/web/dist"
export MANAGEMENT_LISTEN="${MANAGEMENT_LISTEN:-127.0.0.1:21114}"
export MANAGEMENT_ORIGIN="${MANAGEMENT_ORIGIN:-http://$MANAGEMENT_LISTEN}"
export MANAGEMENT_INTERNAL_LISTEN="${MANAGEMENT_INTERNAL_LISTEN:-127.0.0.1:21120}"
if [[ ! -f "$MANAGEMENT_INTERNAL_KEY_FILE" ]]; then
  python3 -c 'import secrets; print(secrets.token_hex(32))' > "$MANAGEMENT_INTERNAL_KEY_FILE"
fi
npm --prefix management/web ci --no-fund --no-audit
npm --prefix management/web run build
bash scripts/cargo-local.sh build --locked --manifest-path management/Cargo.toml
user_count="$(python3 - "$MANAGEMENT_DATABASE" <<'PY'
import pathlib, sqlite3, sys
path = pathlib.Path(sys.argv[1])
if not path.exists():
    print(0)
else:
    with sqlite3.connect(path.as_uri() + '?mode=ro', uri=True) as db:
        print(db.execute('SELECT count(*) FROM users').fetchone()[0])
PY
)"
if [[ "$user_count" == 0 ]]; then
  if [[ ! -f "$runtime_dir/admin-password" ]]; then
    python3 -c 'import secrets; print(secrets.token_urlsafe(24))' > "$runtime_dir/admin-password"
  fi
  management/target/debug/rustdesk-management init-admin admin < "$runtime_dir/admin-password"
fi
printf 'Local administrator: admin\nPassword file: %s/admin-password\n' "$runtime_dir"
exec management/target/debug/rustdesk-management
