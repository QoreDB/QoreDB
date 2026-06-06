<!-- SPDX-License-Identifier: BUSL-1.1 -->

# qore-server

Self-hostable HTTP control plane for QoreDB. Hosts `qore-service`, brokers DB
sessions **server-side** (credentials and sockets never reach the browser), and
serves the web frontend. Enterprise tier (`BUSL-1.1`).

## Status

v0 (mono-utilisateur). Sessions follow the **session-id model** (same as the
desktop): `connect_saved_connection` returns a `session_id`, every other command
carries it. The `SessionManager` is the registry — no per-connection cache.

## Configuration (env)

| Variable             | Default            | Description                                  |
| -------------------- | ------------------ | -------------------------------------------- |
| `QORE_SERVER_HOST`   | `127.0.0.1`        | Listen address. Set to `0.0.0.0` to expose.  |
| `QORE_SERVER_PORT`   | `8088`             | Listen port.                                 |
| `QORE_SERVER_TOKEN`  | _(generated)_      | Bearer token. If unset, one is generated and logged at startup. |
| `QORE_SERVER_WEB_DIR`| _(none)_           | Path to the built frontend (`dist/`). When set, the SPA is served and the token is injected as `window.__QORE_TOKEN__`. |
| `QOREDB_CONFIG_DIR`  | desktop config dir | Vault/config directory (same keyring as the desktop app). |

> The v0 reuses the desktop OS keyring for credentials (like `qore-mcp` /
> `qore-cli`). This requires an OS secret service, so it does **not** work
> headless/Docker yet — an encrypted-file credential provider comes with the
> packaging step.

## API

- `GET  /health` — unauthenticated liveness probe.
- `GET  /api/status` — server status (auth).
- `POST /api/invoke` — generic command bridge mirroring the desktop Tauri
  commands: `{ "command": "...", "args": { ... } }` (auth). Bridged: `list_saved_connections`,
  `connect_saved_connection`, `disconnect`, `list_namespaces`, `list_collections`,
  `describe_table`, `query_table`, `execute_query` (buffered). Unbridged commands
  return `400`.
- `POST /api/stream/execute_query` — streaming query as **SSE** (`columns` / `row`
  / `rows` / `error` / `done` events) (auth).

The frontend talks to this via `src/lib/transport.ts` (`HttpTransport`): in web
mode `invoke()` POSTs to `/api/invoke`, and `executeQuery` streams from
`/api/stream/execute_query`.

## Run

```bash
cargo run -p qore-server
curl http://127.0.0.1:8088/health                          # -> ok
curl -H "Authorization: Bearer $TOKEN" \
     -d '{"command":"list_saved_connections"}' \
     http://127.0.0.1:8088/api/invoke                       # -> [ ...saved connections ]
```

To serve the web app:

```bash
pnpm build
QORE_SERVER_WEB_DIR=./dist cargo run -p qore-server
# open http://127.0.0.1:8088
```
