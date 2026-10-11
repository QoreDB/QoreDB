#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Launch only synthetic profiles from prepare-native-qualification.py.
set -euo pipefail

PROFILE_ROOT="$(realpath "${1:?Usage: $0 profile-directory binary}")"
APP_BINARY="$(realpath "${2:?Usage: $0 profile-directory binary}")"
test -f "$PROFILE_ROOT/fixture-manifest.json"
test -x "$APP_BINARY"
if [ "${QOREDB_QUALIFICATION_BUS:-}" != "1" ]; then
    exec dbus-run-session -- env QOREDB_QUALIFICATION_BUS=1 bash "$0" "$PROFILE_ROOT" "$APP_BINARY"
fi

unset QORE_VAULT_KEY QORE_VAULT_FILE QOREDB_WORKSPACE QOREDB_CONFIG_DIR AT_SPI_BUS_ADDRESS
export XDG_CONFIG_HOME="$PROFILE_ROOT/config"
export XDG_DATA_HOME="$PROFILE_ROOT/data"
export XDG_CACHE_HOME="$PROFILE_ROOT/cache"
export XDG_RUNTIME_DIR="$PROFILE_ROOT/runtime"
export GNOME_KEYRING_CONTROL="$XDG_RUNTIME_DIR/keyring"
export WEBKIT_INSPECTOR_HTTP_SERVER="${QOREDB_QUALIFICATION_INSPECTOR:-127.0.0.1:9333}"
export GDK_BACKEND=x11
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
dbus-update-activation-environment XDG_CONFIG_HOME XDG_DATA_HOME XDG_CACHE_HOME XDG_RUNTIME_DIR

FIRST_KEYRING=false
if [ ! -f "$XDG_DATA_HOME/keyrings/login.keyring" ]; then FIRST_KEYRING=true; fi
printf 'qualification-keyring-only\n' | gnome-keyring-daemon --unlock --components=secrets --control-directory="$GNOME_KEYRING_CONTROL"
printf '%s' "$DBUS_SESSION_BUS_ADDRESS" > "$XDG_RUNTIME_DIR/bus"

if "$FIRST_KEYRING"; then
    python - "$PROFILE_ROOT" <<'PY'
import json
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1])
manifest = json.loads((root / 'fixture-manifest.json').read_text())
assert manifest['baseline'] == '252e0b267b6d67585b6916bce2ce29f516685fd8'
for connection in manifest['connections']:
    subprocess.run([
        'secret-tool', 'store', '--label=QoreDB synthetic qualification',
        'service', 'qoredb_' + connection['project_id'],
        'username', 'creds_' + connection['id'],
        'target', 'default', 'application', 'rust-keyring',
    ], input=json.dumps({
        'db_password': 'synthetic-password-' + connection['id'],
        'ssh_password': None, 'ssh_key_passphrase': None, 'proxy_password': None,
    }), text=True, check=True, timeout=10)
PY
fi
cd "$PROFILE_ROOT"
exec "$APP_BINARY"
