# Quality Plan (Target 9/10 on All Criteria)

Goal: raise architecture/extensibility, reliability, security, observability, and tests/maintenance to 9/10+.

## 1) Trust Boundaries (Security + Reliability)

- [x] Make backend the source of truth for `environment` and `read_only` by loading them from vault metadata whenever a connection is saved.
- [x] Add a `connect_by_id(connection_id)` command that uses vault metadata + credentials; deprecate or restrict `connect(config)` to explicit "unsafe/dev-only" usage.
- [x] Validate all connection inputs server-side (driver, host, port, ssl, env) and normalize `environment` to a strict enum.
- [x] Add a backend policy file (or config) for production safety rules; do not trust UI flags.
- [x] Expose production safety policy in Settings with persisted config; env vars override if set.
- [x] Ensure secrets never reach logs: use a `SecretString`-style wrapper and redact query text where needed.

## 2) Query Cancellation + Timeouts (Reliability + Scalability)

- [x] Introduce a `QueryManager` that tracks `QueryId` -> active query handle per session (support multiple parallel queries).
- [x] Extend commands to return `query_id` on execution and accept `query_id` on cancel (keep backward compatibility if needed).
- [x] Implement driver-level cancellation:
- [x] Postgres: capture backend PID (`SELECT pg_backend_pid()`) on the executing connection; cancel from a separate pool connection with `SELECT pg_cancel_backend(pid)`.
- [x] MySQL: capture connection ID (`SELECT CONNECTION_ID()`) on the executing connection; cancel with `KILL QUERY <id>` (fallback to `KILL CONNECTION` if needed).
- [x] MongoDB: best-effort cancel via task abort + dropping cursor/session; document limitations.
- [x] Add driver capability flags for cancel support (real vs best-effort).
- [x] On timeout, trigger driver cancel + clean up in `QueryManager`.

## 3) SQL Safety + Read-Only Enforcement (Security)

- [x] Replace keyword heuristics with a SQL parser (e.g., `sqlparser`) and a normalized AST-based classifier.
- [x] Correctly classify CTEs (`WITH ... SELECT`) and multi-statement scripts.
- [x] Add production "dangerous" rules to the parser (DROP/ALTER/TRUNCATE/UPDATE-DELETE without WHERE, etc.).
- [x] Add unit tests for the parser with a table of safe/unsafe queries across Postgres/MySQL dialects.

## 4) SSH Password Auth Support (Security + UX)

- [x] Add a pluggable SSH tunnel backend (`SshTunnelBackend` trait).
- [x] Keep OpenSSH backend for key-based auth; add an embedded backend (libssh2/ssh2 crate) for password auth.
- [x] Support host key policy and known_hosts handling in the embedded backend.
- [x] Choose backend automatically based on `SshAuth` (password => embedded, key => openssh by default).
- [x] Add tests for both backends and error handling.

## 5) Observability (Operability)

- [x] Add structured logging with `tracing` (per request + per query + per connection).
- [x] Persist logs to file with rotation.
- [x] Add a UI command to export logs for support.
- [x] Add correlation IDs (session_id, query_id) to logs without leaking secrets.
- [x] Track query durations and cancellations, expose basic metrics in dev builds.

## 6) Tests + CI (Quality & Maintenance)

- [ ] Add integration tests for Postgres/MySQL/Mongo using `docker-compose` and seeded data.
- [ ] Add end-to-end tests for: connect, list namespaces/collections, execute query, cancel query, begin/commit/rollback.
- [ ] Add tests for vault storage (save/list/delete/lock) and for SSH tunnel config.
- [ ] Add CI steps to run unit + integration tests on Linux.

## 7) Driver API + Capabilities (Architecture)

- [x] Add a `DriverCapabilities` struct (transactions, mutations, cancel, supports_ssh, etc.).
- [x] Enforce capability checks in commands and report clear errors to the UI.
- [ ] Document driver-specific limitations and provide consistent fallback behavior.

## 8) Documentation + Release Readiness

- [x] Update `doc/PROJECT.md` and `doc/FEATURES.md` with the new backend trust model.
- [x] Document the cancel model and its limits per driver.
- [x] Add a security note explaining production safeguards and vault trust boundaries.
