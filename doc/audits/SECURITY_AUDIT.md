# Security Audit Report

> **Status:** Reviewed, targeted refresh
> **Original date:** 2026-04-05
> **Last reviewed:** 2026-07-08
> **Code baseline:** `c65885d6c9032f1f8825e194fde85f8b38bfd1a1`
> **Review depth:** Targeted refresh of previously documented findings; not a
> full external penetration test.
> **Project:** QoreDB

## Executive Summary

QoreDB's current security posture is materially better than the April 2026
snapshot. The highest-risk items from the previous version of this audit have
been addressed: raw audit/profiling persistence is redacted, specialized
database-create/drop commands now enforce read-only mode, custom share providers
reject non-localhost plaintext HTTP, filesystem access is positively scoped, and
browser/table endpoints now use shared governance helpers for row limits,
concurrency checks, and timeouts.

The main remaining risk is local persistence of user work state. Crash recovery
has a TTL and a query-draft opt-out, but the default still stores raw query
drafts in `localStorage` for recovery. This is a reasonable product trade-off
for a local-first desktop app, but it should be presented as an explicit privacy
choice and should be covered by the GDPR audit refresh.

## Targeted Review Evidence

Commands and checks used during the 2026-07-08 refresh:

- `rg "dangerouslySetInnerHTML|eval\\(|new Function" src src-tauri` returned no
  matches.
- `rg "localStorage|sessionStorage" src src-tauri` confirmed continued local
  persistence usage, including crash recovery, query history, notebook drafts,
  diagnostics, UI preferences, and session tokens.
- `rg "is_read_only|redact_query|validate_share_url" src-tauri/src src-tauri/crates`
  confirmed current code references for read-only gates, redaction, and share
  URL validation.

No compiling test suite was completed during this refresh pass.

## Findings

### 1. Tauri Configuration

- **Resolved: CSP is configured**
  - **Location:** `src-tauri/tauri.conf.json`
  - **Current state:** The app defines a CSP with explicit `default-src`,
    `script-src`, `style-src`, `img-src`, `font-src`, and `connect-src`
    allowlists.
  - **Residual note:** `style-src 'unsafe-inline'` remains present, which is a
    common Tauri/React compromise but should not be expanded casually.
    `connect-src` is limited to Tauri IPC/localhost endpoints; the PostHog hosts
    were removed together with the telemetry integration.
  - **Assessment:** No active finding.

- **Resolved: Filesystem permissions are positively scoped**
  - **Location:** `src-tauri/capabilities/default.json`
  - **Current state:** `fs:scope` allows `$APPCONFIG`, `$APPDATA`,
    `$APPLOCALDATA` and their subtrees, with a deny-list for sensitive paths as
    defense in depth.
  - **Residual note:** `opener:allow-open-path` permits opening broader
    user-facing locations such as home, documents, downloads, and desktop. This
    is not equivalent to filesystem read/write permission, but it should remain
    intentionally reviewed if opener usage grows.
  - **Assessment:** Previous broad-write concern is closed.

### 2. Backend Safety Enforcement

- **Resolved: Read-only mode covers specialized mutation commands**
  - **Locations:** `src-tauri/src/commands/query.rs`,
    `src-tauri/src/commands/import.rs`, `src-tauri/src/commands/maintenance.rs`,
    `src-tauri/src/commands/routines.rs`, `src-tauri/src/commands/sequences.rs`,
    `src-tauri/src/commands/triggers.rs`,
    `src-tauri/crates/qore-service/src/mutation.rs`,
    `src-tauri/crates/qore-service/src/query.rs`
  - **Current state:** `create_database` and `drop_database` now return
    `Operation blocked: read-only mode` before driver dispatch. Import,
    mutation, maintenance, routines, triggers, sequences, sandbox, and service
    query paths also check read-only mode.
  - **Assessment:** The previous high-risk finding is closed. Future mutating
    commands should use the same hard gate before side effects.

- **Resolved: Governance limits cover browse endpoints**
  - **Locations:** `src-tauri/crates/qore-service/src/governance.rs`,
    `src-tauri/crates/qore-service/src/query.rs`,
    `src-tauri/src/commands/query.rs`
  - **Current state:** `preview_table` clamps requested rows via
    `governance::clamp_rows`, checks concurrent-query limits, and runs the
    driver future through `governance::with_timeout`. `query_table` clamps page
    size, checks concurrency, and applies timeout. `peek_foreign_key` clamps the
    tooltip limit, checks concurrency, and applies timeout.
  - **Residual note:** Cached table-browse responses can return before
    concurrency/timeout checks, which is acceptable because no driver work is
    started. Cache correctness and invalidation belong to cache-specific audits.
  - **Assessment:** The previous medium-risk finding is closed.

- **Managed risk: Governance bypass is privileged and capped**
  - **Location:** `src-tauri/src/commands/query.rs`
  - **Current state:** `bypass_limits=true` is rejected unless the effective
    license tier includes Team. Even with bypass granted, query execution keeps
    an absolute timeout cap of 1 hour.
  - **Assessment:** No active finding, but this path should remain covered by
    regression tests because it intentionally weakens normal guardrails.

### 3. Query Redaction and Observability

- **Resolved: Raw queries are redacted before audit persistence**
  - **Locations:** `src-tauri/crates/qore-service/src/interceptor/types.rs`,
    `src-tauri/crates/qore-service/src/interceptor/redaction.rs`
  - **Current state:** `AuditLogEntry::new` calls `redact_query` before storing
    `query` and `query_preview`, then computes a stable fingerprint from the
    redacted form.
  - **Assessment:** Previous raw audit persistence finding is closed.

- **Resolved: Slow-query profiling stores redacted queries**
  - **Location:** `src-tauri/crates/qore-service/src/interceptor/profiling.rs`
  - **Current state:** `record_slow_query` calls `redact_query` before adding
    slow-query entries.
  - **Residual note:** Profiling percentiles are recomputed from bounded
    in-memory samples and should be treated as operational indicators, not
    authoritative historical analytics.
  - **Assessment:** Redaction concern is closed; metrics caveat remains low
    risk/documentation only.

- **Resolved: Audit export can read retained disk history**
  - **Location:** `src-tauri/crates/qore-service/src/interceptor/audit.rs`
  - **Current state:** `get_entries_from_disk` exists and tests cover reading
    all disk entries even when the in-memory cache is truncated.
  - **Assessment:** Previous export-retention concern is closed.

### 4. Frontend and Local Persistence

- **Medium Risk: Crash recovery stores raw query drafts by default**
  - **Locations:** `src/lib/diagnostics/crashRecovery.ts`,
    `src/lib/diagnostics/crashRecoverySettings.ts`,
    `src/providers/SessionProvider.tsx`
  - **Current state:** Recovery snapshots are stored in `localStorage` under
    `qoredb_crash_recovery`. Snapshots include tab metadata and, when
    `saveQueryDrafts` is enabled, raw query drafts. The default is
    `saveQueryDrafts: true` with `ttlHours: 24`; stale snapshots are discarded
    during normalization.
  - **Implication:** Sensitive ad hoc queries can remain on disk after a crash
    even when query history persistence is disabled.
  - **Recommendation:** Make the privacy trade-off explicit in settings and
    onboarding copy, consider defaulting query-draft recovery off for production
    connections, or redact query drafts before persistence.

- **Low Risk: Notebook drafts also persist locally**
  - **Location:** `src/lib/notebook/notebookIO.ts`
  - **Current state:** Notebook drafts are saved under `qnb_draft_<tabId>` in
    `localStorage`.
  - **Implication:** Notebook SQL/markdown can contain sensitive information and
    is not covered by the crash-recovery TTL.
  - **Recommendation:** Include notebook drafts in the GDPR/local persistence
    inventory and provide a clear cleanup path.

- **Low Risk: Query history and error logs are safer than recovery state**
  - **Locations:** `src/lib/query/history.ts`,
    `src/lib/diagnostics/errorLog.ts`,
    `src/lib/diagnostics/diagnosticsSettings.ts`
  - **Current state:** Query history persists only when diagnostics settings
    allow it; persisted query text is redacted. Error logs redact messages and
    details before storage.
  - **Assessment:** This is an acceptable baseline. The remaining issue is
    consistency: recovery and notebook drafts are less privacy-preserving than
    history/error-log persistence.

### 5. Sharing

- **Resolved: Custom share providers require HTTPS except loopback**
  - **Location:** `src-tauri/src/share/manager.rs`
  - **Current state:** Upload URLs and returned share URLs must use `https://`
    unless the host is loopback (`localhost`, `127.0.0.1`, `::1`, `[::1]`).
  - **Assessment:** Previous plaintext share-provider finding is closed.

### 6. Frontend Code Injection Surface

- **No active finding**
  - **Evidence:** No `dangerouslySetInnerHTML`, `eval(`, or `new Function`
    matches were found under `src` or `src-tauri`.
  - **Assessment:** The app continues to rely on backend-first Tauri commands
    for critical operations.

## Recommendations

1. Treat crash recovery and notebook drafts as the next security/privacy work
   item: document the behavior, add clearer user controls, and consider
   redaction or production-connection defaults.
2. Keep new mutating Tauri commands behind read-only checks before driver calls.
3. Keep all new data-access endpoints wired to `governance::clamp_rows`,
   `governance::check_concurrent_limit`, and `governance::with_timeout`.
4. Add targeted regression tests around `bypass_limits` license gating and the
   1-hour absolute timeout cap.
5. Continue regular dependency checks with `pnpm audit`, `cargo audit`, and
   `cargo deny check` in CI.

## Conclusion

The April 2026 audit's most serious active findings are now resolved. QoreDB is
best described as security-aware with a strong backend enforcement baseline and
one clear remaining local-privacy gap: raw recovery/draft persistence. The next
deep audit should focus less on classic web injection and more on local data
lifecycle, user-visible privacy controls, and regression tests for safety
invariants.
