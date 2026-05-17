# QoreDB v0.1.28 — Data Quality & Integration

## ✨ Highlights

This release turns QoreDB into a data-quality dashboard and the easiest way to expose your data elsewhere: two flagship Pro features (Data Contracts, Instant Data API), one new analytics driver (ClickHouse), and three Core polish items (customizable shortcuts, backup/restore helpers, audit hardening with query fingerprinting).

## 🎯 New features

### Data Contracts [Pro]

Declarative, executable data-quality assertions persisted as YAML in the workspace.

- 12 rule types covering presence, format, domain, uniqueness, referential integrity, cardinality, and arbitrary `custom_sql` assertions.
- All rules execute as generated SQL against the target connection — no raw download of the data.
- Workspace storage: `.qoredb/contracts/<name>.yml`, with a JSONL history of past runs.
- Live progress events (`contract.run.progress` / `completed`) and a contract-health badge in the sidebar.
- New notebook cell type `'contract'` to embed a contract run inline in a notebook.
- Post-mutation hook: when a mutation hits a table covered by an active contract, the contract is re-evaluated asynchronously (best-effort, non-blocking).

### Instant Data API [Pro]

Expose saved queries as **read-only** REST endpoints on `127.0.0.1`.

- Bind strict to loopback (never `0.0.0.0`).
- Bearer token per endpoint, auto-generated (`api-` + 32 random bytes), shown once at creation, Argon2-hashed at rest. **Regenerate** action included.
- `Read`-classification gate on every request — mutations are rejected even if the saved query was rewritten after creation.
- Default port `4787` ("QORE" on a phone keypad), rate limit 10 req/s per endpoint, pagination (`{ data, page, total }`) for the `rows` shape.
- Built-in routes: `GET /health` (public) and `GET /openapi.json` (OpenAPI 3.1, auth-gated).
- Lifecycle: explicit start/stop, drops on App Lock / workspace switch.

### Customizable Keyboard Shortcuts [Core]

Every keyboard binding is now editable from **Settings → Shortcuts**.

- Click-to-rebind recorder with conflict detection.
- Cross-OS chord syntax (`Mod` = Cmd on macOS, Ctrl elsewhere).
- System-reserved chords (`Mod+Tab`, `Mod+Q`, `F11`/`F12`, naked `Enter`/`Space`) explicitly refused.
- Reset per shortcut or globally; persistence stored in the workspace.

### Backup / Restore Helpers [Core]

Visual wrappers around the native dump tools, with explicit binary detection and per-tool path overrides.

- **Backup**: `pg_dump`, `mysqldump` / `mariadb-dump`, `mongodump`, `sqlite3 .dump`.
- **Restore**: `pg_restore` / `psql`, `mysql` / `mariadb`, `mongorestore`, `sqlite3 <`.
- Streaming stdout/stderr to the UI (ring buffer 1000 lines), cancellable mid-run.
- Double-confirmation pattern for restore (typed DB name), output paths always chosen via picker.
- Detect tools in `$PATH`; per-tool override picker in **Settings → Data → Backup tools**.

### Audit & Security Hardening [Core]

Five findings resolved from the April 2026 security audit, plus query fingerprinting.

- **Read-only uniformity** — every mutating command (`mutation`, `maintenance`, `routines`, `triggers`, `sequences`, `create_database`, `drop_database`) gates on `is_read_only`.
- **Governance limits extended** to `preview_table`, `query_table`, `peek_foreign_key` (timeout, max_rows, concurrent queries).
- **Audit log read-from-disk** — `export_audit_log(format, from_disk)` now reads the full retained trail (JSON / JSONL / CSV).
- **Query fingerprinting** — every audit entry carries a stable SHA-256 fingerprint of the normalized query (literals → `?`, identifiers preserved). Filter and group by fingerprint in the Interceptor panel.
- **Share providers** — HTTPS enforced (with explicit loopback exception).

### Driver ClickHouse [Core]

Native ClickHouse driver wired into the standard `DriverRegistry`.

- HTTP / HTTPS protocol (8123 / 8443), Rustls TLS, basic auth.
- 28 types mapped: `Int8..Int128`, `UInt*`, `Float32/64`, `Decimal`, `String`, `FixedString`, `Date(32)`, `DateTime(64)`, `Bool`, `UUID`, `Enum8/16`, `Array`, `Tuple`, `Map`, `IPv4/6`.
- AST safety classification with a 4-level model (Read / Mutation / Dangerous / Unknown) — refuses `OPTIMIZE … FINAL`, `KILL QUERY`, `SYSTEM` operations.
- Cancel via `KILL QUERY WHERE query_id = …` (best-effort).
- DDL editor support: backtick identifiers, CHECK constraints, `INDEX … TYPE bloom_filter|minmax|set`.

## 🛠 Under the hood

- New module `src-tauri/src/api/` (Pro, BUSL-1.1) — `axum` server + endpoint registry + Argon2 auth + token bucket rate limiter + OpenAPI 3.1 generator.
- New module `src-tauri/src/contracts/` (Pro, BUSL-1.1) — parser, runner, dialect-aware SQL builders, post-mutation alert hook.
- New module `src-tauri/src/backup/` (Core) — tool detection, argument builders with `safe_identifier` validation, streaming runner, cancellable.
- New module `src-tauri/src/interceptor/fingerprint.rs` (Core) — multi-dialect query normalization (SQL / Mongo / Redis) + SHA-256.
- New module `src-tauri/src/interceptor/export.rs` (Core) — JSON / JSONL / CSV writers.
- New module `src-tauri/src/api/openapi.rs` (Pro) — OpenAPI 3.1 document generation.
- New crate folder `qore-drivers/src/drivers/clickhouse/` (Core) — `client`, `types`, `response`, `describe`, `driver`, safety classification.
- New PostHog events documented in `doc/release/EVENTS.md` (`contract_*`, `instant_api_*`, `backup_*`, `restore_*`, `shortcut_*`, `audit_exported`, `audit_filtered_by_fingerprint`).
- 9 locales updated (en, fr, de, es, pt-BR, ru, ja, ko, zh-CN).

## ⚠️ Known limitations

- **Data Contracts** — `custom_sql` runs against the target connection only; no federation in v1. Planned for v0.1.29.
- **Instant Data API** — `127.0.0.1` only, no HTTPS in v1 (justified by loopback). No WebSocket, no write endpoints.
- **Backup / Restore** — DuckDB, SQL Server, Redis, ClickHouse not covered in v1. Explicit "not available for this driver" message in the UI.
- **ClickHouse** — DDL Alter UI limited to MergeTree-family subset; no `ON CLUSTER` support in v1.
- **Customizable Shortcuts** — system-reserved chord list is best-effort on Linux (varies by window manager).
- **Filesystem capability scope** — `default.json` now ships a sensitive-path deny-list (`.ssh`, `.aws`, `.kube`, `/etc`, …). Switch to a positive allow-list (`$DOCUMENT/qoredb/*`, `$DOWNLOAD/*`, `$APPDATA/qoredb/*`, `$HOME/.qoredb/*`) is deferred to v0.1.29 to avoid regressing exports / notebooks / blob downloads.

## 📦 Driver matrix (additions)

| Driver | DDL Create | DDL Alter | Bulk Edit | Backup | Contracts |
| ------ | :--------: | :-------: | :-------: | :----: | :-------: |
| ClickHouse | ✅ | ⚠️ MergeTree subset | ✅ | ❌ v1 | ✅ |

---

**Full changelog**: `git log v0.1.27..v0.1.28`
