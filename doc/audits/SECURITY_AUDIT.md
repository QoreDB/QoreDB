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
  - **Finding:** The frontend has write access through `fs:allow-write-text-file` and `fs:allow-write-file`, plus text-file read access, without an explicit path scope in the capability file.
  - **Implication:** This is partly required for notebooks, exports, imports, and log saves, but it still expands the blast radius of any future frontend compromise.
  - **Recommendation:** Reduce scope where Tauri plugin support allows it, or document that access is intentionally user-path based and mediated by file pickers.

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

- **Medium Risk: Raw queries are stored by audit and profiling**
  - **Location:** `src-tauri/src/interceptor/*`
  - **Finding:** Audit entries and slow-query entries persist the original query string, not a redacted version.
  - **Implication:** literals, secrets typed into queries, temporary tokens, or sensitive identifiers may be written to local audit/profiling storage.
  - **Recommendation:** Add backend-side redaction before persistence, or make the raw-query storage behavior explicit in product documentation and UI.

- **Medium Risk: Custom share providers allow plaintext HTTP**
  - **Location:** `src-tauri/src/share/manager.rs`
  - **Finding:** custom share upload URLs and returned share URLs currently accept both `http` and `https`.
  - **Implication:** exports and optional bearer tokens can be transmitted without transport encryption.
  - **Recommendation:** require `https` by default, with an explicit localhost-only exception if needed for development.

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

- **Medium Risk: Audit retention and export semantics are misleading**
  - **Location:** `src-tauri/src/interceptor/audit.rs`
  - **Finding:** `max_audit_entries` governs file rotation, but `get_audit_entries()` and `export_audit_log()` only operate on the 1000-entry in-memory cache.
  - **Implication:** operators can believe they are browsing/exporting the full retained audit trail when they are not.
  - **Recommendation:** either load from disk for export/history views or document the distinction clearly in the product and internal docs.

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

1. Enforce read-only mode uniformly across all mutating commands, including specialized DDL helpers.
2. Redact query text before writing audit entries and slow-query entries.
3. Clarify and/or reduce local persistence of raw drafts in crash recovery.
4. Restrict custom share providers to `https` by default.
5. Align governance documentation with actual runtime scope, or extend scope to browser endpoints.
6. Clarify audit retention/export semantics and tighten filesystem capability scope where possible.
7. Continue regular `pnpm audit`, `cargo audit`, and `cargo deny` checks in CI.

## Conclusion

The CSP issue documented in the January 22, 2026 audit is no longer present. QoreDB is now better described as security-aware but still carrying several local-data-handling and policy-consistency gaps that should be addressed before the documentation can claim uniform protection across all execution paths.
