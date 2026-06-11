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
| `QORE_SERVER_WEB_DIR`| _(none)_           | Path to the built frontend (`dist/`). When set, the SPA is served (no token is injected — only the `window.__QORE_WEB__` flag). |
| `QORE_SERVER_TLS_CERT` | _(none)_         | Path to a PEM certificate (full chain). Enables HTTPS; must be set together with `QORE_SERVER_TLS_KEY`. |
| `QORE_SERVER_TLS_KEY`  | _(none)_         | Path to the PEM private key for the certificate. |
| `QOREDB_CONFIG_DIR`  | desktop config dir | Connection metadata directory (`connections.json`).          |
| `QORE_VAULT_KEY`     | _(none)_           | When set, credentials are stored in an **encrypted file** (XChaCha20Poly1305, key derived via Argon2id) instead of the OS keyring — required for headless/Docker. |
| `QORE_VAULT_FILE`    | `<data_dir>/vault.enc` | Path of the encrypted credential file (used only when `QORE_VAULT_KEY` is set). |

> By default credentials use the OS keyring (like `qore-mcp` / `qore-cli`), which
> needs an OS secret service and does **not** work headless. For Docker, set
> `QORE_VAULT_KEY` (and optionally `QORE_VAULT_FILE`) to switch to the encrypted
> file provider. Losing `QORE_VAULT_KEY` makes the stored credentials
> unrecoverable.

## Docker

```bash
export QORE_SERVER_TOKEN=$(openssl rand -hex 24)
export QORE_VAULT_KEY=$(openssl rand -hex 32)
docker compose -f docker-compose.server.yml up --build
# open http://127.0.0.1:8088
```

The image (`Dockerfile` at the repo root) builds the SPA and the server, serves
the frontend from `/app/web`, and persists `connections.json` + `vault.enc` to
the `/data` volume. Credentials are encrypted with `QORE_VAULT_KEY`.

## Authentication

Two principals reach the API:

- **Admin token** — the shared `QORE_SERVER_TOKEN` (machine / break-glass / CI). Full access, never exposed to the browser.
- **Users** — accounts in the control plane (`<config_dir>/control.db`), authenticated by JWT. Credentials are **never** seeded from the environment: the first admin is created through the bootstrap register flow.

Web auth flow (public endpoints, no token required):

- `GET  /api/auth/status` → `{ "setupRequired": bool }` — `true` while no user exists, so the UI routes to register vs login.
- `POST /api/auth/register` `{ email, password }` — creates the **first admin**. Allowed only while the instance has zero users; returns `403` afterwards (further users come from `POST /api/admin/users`).
- `POST /api/auth/login` `{ email, password }` → `{ token, email, isAdmin }`. The JWT is sent as `Authorization: Bearer <token>` on subsequent calls.

RBAC: a user only sees and connects to the connections granted to its roles; a `read` grant forces the connection read-only. Admin provisioning: `POST /api/admin/{users,roles,assign,grants}`, `GET /api/admin/users` (admin only).

### SSO / OIDC (optional)

Set all four variables to enable OpenID Connect (Authorization Code + PKCE):

| Variable | Description |
| --- | --- |
| `QORE_OIDC_ISSUER` | IdP issuer URL (e.g. `https://keycloak.example/realms/qore`). |
| `QORE_OIDC_CLIENT_ID` | Registered client id. |
| `QORE_OIDC_CLIENT_SECRET` | Client secret. |
| `QORE_OIDC_REDIRECT_URI` | Must equal `<public-url>/api/auth/oidc/callback`. |

Discovery runs at boot; if it fails the server still starts with SSO disabled. `GET /api/auth/status` reports `ssoEnabled`. Flow: `GET /api/auth/oidc/start` → 302 to the IdP; the IdP calls back `GET /api/auth/oidc/callback`, which validates the id_token against the IdP JWKS (signature, issuer, audience, expiry, nonce), **JIT-provisions** an unknown email as a non-admin user with no grants (an admin then assigns roles), mints the app JWT, and bounces the browser to `/?sso_token=<jwt>` (or `/?sso_error=<code>`).

## API

- `GET  /health` — unauthenticated liveness probe.
- `GET  /api/status` — server status (auth).
- `POST /api/invoke` — generic command bridge mirroring the desktop Tauri
  commands: `{ "command": "...", "args": { ... } }` (auth). Bridged: `list_saved_connections`,
  `connect_saved_connection`, `disconnect`, `get_driver_info`, `list_namespaces`,
  `list_collections`, `describe_table`, `query_table`, `execute_query` (buffered).
  Unbridged commands return `400`.
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

## TLS

Terminate TLS in the server by pointing it at a PEM certificate/key pair
(rustls — no OpenSSL):

```bash
QORE_SERVER_TLS_CERT=/path/fullchain.pem \
QORE_SERVER_TLS_KEY=/path/privkey.pem \
cargo run -p qore-server
# open https://127.0.0.1:8088
```

Setting only one of the two variables is a configuration error and the server
exits at boot. Without them the server speaks plain HTTP (put it behind a
TLS-terminating reverse proxy, or use these variables for direct exposure).
