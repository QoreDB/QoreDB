# Security Audit Report

**Date:** April 5, 2026
**Project:** QoreDB

## Executive Summary

QoreDB follows a solid baseline for a desktop database client: credentials are stored via the OS keyring, the Tauri CSP is configured, and the critical execution path is largely implemented in Rust. The main remaining risks are now less about classic web issues and more about consistency and defense in depth:

- raw query text is persisted by the interceptor audit/profiling pipeline
- read-only guarantees are not uniformly enforced across every specialized command path
- crash recovery stores raw drafts in `localStorage`
- custom share providers currently allow plaintext `http`
- governance limits are enforced on `execute_query` but not uniformly on table-browser endpoints

## Findings

### 1. Tauri Configuration

- **Resolved Since January 22, 2026: CSP is configured**
  - **Location:** `src-tauri/tauri.conf.json`
  - **Current state:** The app now defines a CSP with explicit `default-src`, `script-src`, `style-src`, `img-src`, `font-src`, and `connect-src` allowlists.
  - **Assessment:** This closes the previously documented `csp: null` issue and materially improves resistance to webview content injection.

- **Medium Risk: Filesystem permissions remain broad**
  - **Location:** `src-tauri/capabilities/default.json`
  - **Finding:** The frontend has write access through `fs:allow-write-text-file` and `fs:allow-write-file`, plus text-file read access, with a deny-list for sensitive locations (`.ssh`, `.aws`, `.kube`, history files, `/etc`, `/root`, …) but no positive allow-list.
  - **Implication:** This is partly required for notebooks, exports, imports, and log saves; the deny-list prevents the worst exfiltration paths but still expands the blast radius of any future frontend compromise.
  - **Status (v0.1.28):** Deny-list shipped in v0.1.28 (`fs:scope` block in `default.json`). Tightening to a positive allow-list (`$DOCUMENT/qoredb/*`, `$DOWNLOAD/*`, `$APPDATA/qoredb/*`, `$HOME/.qoredb/*`) is **deferred to v0.1.29** — it requires a path-by-path audit of exports / notebooks / blob downloads to avoid regressions, which is out of scope for v0.1.28.
  - **Recommendation:** Plan the allow-list switch alongside the v0.1.29 sandbox work.

### 2. Backend Safety Enforcement

- **High Risk: Read-only mode is not enforced uniformly**
  - **Location:** `src-tauri/src/commands/query.rs`
  - **Finding:** `execute_query` blocks mutations in read-only mode, but specialized commands such as `create_database` and `drop_database` read the flag without rejecting the operation.
  - **Implication:** The repository documents read-only as a backend guarantee, but that guarantee is currently incomplete outside the main query path.
  - **Recommendation:** Apply the same hard block used by `execute_query`, `mutation`, `maintenance`, `routines`, `triggers`, and `sequences` to every mutating command.

- **Medium Risk: Governance limits do not cover all data-access paths**
  - **Location:** `src-tauri/src/commands/query.rs`
  - **Finding:** timeout, concurrent-query, and row-truncation safeguards are applied to `execute_query`, but not to `preview_table`, `query_table`, or `peek_foreign_key`.
  - **Implication:** Large browser-driven reads can bypass limits that users may believe are global.
  - **Recommendation:** Either extend governance to those commands or narrow the documentation so the scope is explicit.

- **Resolved Since May 9, 2026: Raw queries are redacted before persistence**
  - **Location:** `src-tauri/src/interceptor/redaction.rs`, `src-tauri/src/interceptor/types.rs`
  - **Current state:** `AuditLogEntry::new` runs `redact_query` (driver-aware: SQL string literals + connection URIs, MongoDB sensitive fields, Redis `AUTH`/`CONFIG SET`/`EVAL` arguments). The same path also computes a stable fingerprint to enable grouping without exposing literals.

- **Resolved Since May 9, 2026: Custom share providers require HTTPS**
  - **Location:** `src-tauri/src/share/manager.rs` (`validate_provider_config`, `validate_share_url`)
  - **Current state:** Upload URLs and returned share URLs are rejected unless they use `https://`, with an explicit loopback exception (`localhost`, `127.0.0.1`, `::1`) for local development.

### 3. Frontend and Local Persistence

- **Medium Risk: Crash recovery stores raw drafts in localStorage**
  - **Location:** `src/providers/SessionProvider.tsx`, `src/lib/crashRecovery.ts`
  - **Finding:** query drafts are saved automatically for recovery and are not redacted.
  - **Implication:** sensitive ad hoc queries can persist on disk even when query history/error-log retention is disabled.
  - **Recommendation:** add a dedicated opt-in/opt-out, retention window, or redaction strategy for recovery snapshots.

- **Low Risk: Query history and error logs are safer than recovery state**
  - **Location:** `src/lib/history.ts`, `src/lib/errorLog.ts`
  - **Finding:** history and error logs are gated by diagnostics settings and are redacted before persistence.
  - **Assessment:** this is a good baseline, but it currently does not extend to crash recovery or interceptor storage.

### 4. Observability Reliability

- **Resolved Since May 9, 2026: Audit export now reads the full retained trail from disk**
  - **Location:** `src-tauri/src/interceptor/audit.rs` (`get_entries_from_disk`, `export_format`), `src-tauri/src/interceptor/export.rs`
  - **Current state:** `export_audit_log` accepts a `format` (`json` / `jsonl` / `csv`) and a `from_disk` flag. When `from_disk = true`, the rotated `audit.jsonl` is streamed in full — no longer bounded by the in-memory cache. The cache stays available for fast browsing, the export reflects retention.

- **Low Risk: Profiling percentiles should be treated as indicative**
  - **Location:** `src-tauri/src/interceptor/profiling.rs`
  - **Finding:** percentiles are computed from a bounded in-memory sample of execution times.
  - **Implication:** percentiles are useful operational signals, but should not be treated as authoritative historical analytics.
  - **Recommendation:** document that metrics are bounded-window telemetry.

### 5. Frontend Security

- **Code Quality**
  - **Finding:** No usage of `dangerouslySetInnerHTML`, `eval()`, or `new Function()` was found in the source code.
  - **Finding:** The application uses a backend-first Tauri invoke model for critical operations.

## Recommendations

1. ~~Enforce read-only mode uniformly across all mutating commands, including specialized DDL helpers.~~ **Resolved (v0.1.28)** — `mutation`, `maintenance`, `routines`, `triggers` (incl. `toggle_trigger` and `drop_event`), `sequences`, `create_database`, `drop_database` all gate on `is_read_only` before dispatching to the driver.
2. ~~Redact query text before writing audit entries and slow-query entries.~~ **Resolved (v0.1.28)** — `AuditLogEntry::new` redacts before persistence; fingerprint computed from the redacted form.
3. Clarify and/or reduce local persistence of raw drafts in crash recovery.
4. ~~Restrict custom share providers to `https` by default.~~ **Resolved (v0.1.28)** — HTTPS enforced with explicit loopback exception.
5. Align governance documentation with actual runtime scope, or extend scope to browser endpoints.
6. ~~Clarify audit retention/export semantics~~ **Resolved (v0.1.28)** — export reads from disk on demand. ~~tighten filesystem capability scope where possible.~~ Pending.
7. Continue regular `pnpm audit`, `cargo audit`, and `cargo deny` checks in CI.

## Conclusion

The CSP issue documented in the January 22, 2026 audit is no longer present. QoreDB is now better described as security-aware but still carrying several local-data-handling and policy-consistency gaps that should be addressed before the documentation can claim uniform protection across all execution paths.
