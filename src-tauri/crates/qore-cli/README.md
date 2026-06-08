<!-- SPDX-License-Identifier: Apache-2.0 -->

# qore (CLI)

Scriptable command-line access to QoreDB. Queries the database connections you
already saved in the desktop app, reusing the same engine (`qore-service`), vault
and safety gates. Useful for CI/CD, headless servers, and quick terminal checks.

## How it works

- Connections + credentials come from the **same OS keyring and config dir as the
  desktop app** (`VaultStorage`, project `default`). No extra setup.
- Queries go through the existing safety engine (prod-dangerous blocking, rate
  limits, etc.). Connections keep their saved `read_only` flag.
- Output is **JSON on stdout**; errors go to stderr with a non-zero exit code.

## Commands

```bash
qore connections                                   # list saved connections
qore query <connection_id> "<sql>"                 # run a query, print rows
qore tables <connection_id> <database> [--schema s] # list tables/collections
qore describe <connection_id> <database> <table> [--schema s]
```

`QOREDB_CONFIG_DIR` overrides the config directory (defaults to the desktop app's).

## Build

```bash
cargo build -p qore-cli --release        # -> target/release/qore
```
