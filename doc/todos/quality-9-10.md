# Quality Plan (Target 9/10 on All Criteria)

Goal: raise architecture/extensibility, reliability, security, observability, and tests/maintenance to 9/10+.

## 1) Trust Boundaries (Security + Reliability)

- [ ] Make backend the source of truth for `environment` and `read_only` by loading them from vault metadata whenever a connection is saved.
- [ ] Add a `connect_by_id(connection_id)` command that uses vault metadata + credentials; deprecate or restrict `connect(config)` to explicit "unsafe/dev-only" usage.
- [ ] Validate all connection inputs server-side (driver, host, port, ssl, env) and normalize `environment` to a strict enum.
- [ ] Add a backend policy file (or config) for production safety rules; do not trust UI flags.
- [ ] Ensure secrets never reach logs: use a `SecretString`-style wrapper and redact query text where needed.

## 2) Query Cancellation + Timeouts (Reliability + Scalability)

- [ ] Introduce a `QueryManager` that tracks `QueryId` -> active query handle per session (support multiple parallel queries).
- [ ] Extend commands to return `query_id` on execution and accept `query_id` on cancel (keep backward compatibility if needed).
- [ ] Implement driver-level cancellation:
- [ ] Postgres: capture backend PID (`SELECT pg_backend_pid()`) on the executing connection; cancel from a separate pool connection with `SELECT pg_cancel_backend(pid)`.
- [ ] MySQL: capture connection ID (`SELECT CONNECTION_ID()`) on the executing connection; cancel with `KILL QUERY <id>` (fallback to `KILL CONNECTION` if needed).
- [ ] MongoDB: best-effort cancel via task abort + dropping cursor/session; document limitations.
- [ ] Add driver capability flags for cancel support (real vs best-effort).
- [ ] On timeout, trigger driver cancel + clean up in `QueryManager`.

## 3) SQL Safety + Read-Only Enforcement (Security)

- [ ] Replace keyword heuristics with a SQL parser (e.g., `sqlparser`) and a normalized AST-based classifier.
- [ ] Correctly classify CTEs (`WITH ... SELECT`) and multi-statement scripts.
- [ ] Add production "dangerous" rules to the parser (DROP/ALTER/TRUNCATE/UPDATE-DELETE without WHERE, etc.).
- [ ] Add unit tests for the parser with a table of safe/unsafe queries across Postgres/MySQL dialects.

## 4) SSH Password Auth Support (Security + UX)

Avant toute chose sur ce point il faut bien regarder tout ce qui a déjà été fait coté SSH.

- [ ] Add a pluggable SSH tunnel backend (`SshTunnelBackend` trait).
- [ ] Keep OpenSSH backend for key-based auth; add an embedded backend (libssh2/ssh2 crate) for password auth.
- [ ] Support host key policy and known_hosts handling in the embedded backend.
- [ ] Choose backend automatically based on `SshAuth` (password => embedded, key => openssh by default).
- [ ] Add tests for both backends and error handling.

## 5) Observability (Operability)

- [ ] Add structured logging with `tracing` (per request + per query + per connection).
- [ ] Persist logs to file with rotation, and add a UI command to export logs for support.
- [ ] Add correlation IDs (session_id, query_id) to logs without leaking secrets.
- [ ] Track query durations and cancellations, expose basic metrics in dev builds.

## 6) Tests + CI (Quality & Maintenance)

- [ ] Add integration tests for Postgres/MySQL/Mongo using `docker-compose` and seeded data.
- [ ] Add end-to-end tests for: connect, list namespaces/collections, execute query, cancel query, begin/commit/rollback.
- [ ] Add tests for vault storage (save/list/delete/lock) and for SSH tunnel config.
- [ ] Add CI steps to run unit + integration tests on Linux.

## 7) Driver API + Capabilities (Architecture)

- [ ] Add a `DriverCapabilities` struct (transactions, mutations, cancel, supports_ssh, etc.).
- [ ] Enforce capability checks in commands and report clear errors to the UI.
- [ ] Document driver-specific limitations and provide consistent fallback behavior.

## 8) Documentation + Release Readiness

- [ ] Update `doc/PROJECT.md` and `doc/FEATURES.md` with the new backend trust model.
- [ ] Document the cancel model and its limits per driver.
- [ ] Add a security note explaining production safeguards and vault trust boundaries.
