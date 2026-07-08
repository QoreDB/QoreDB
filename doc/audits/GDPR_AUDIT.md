# GDPR Compliance Audit - QoreDB

> **Status:** Reviewed, targeted refresh
> **Original date:** 2025-02-21
> **Last reviewed:** 2026-07-08
> **Code baseline:** `c65885d6c9032f1f8825e194fde85f8b38bfd1a1`
> **Review depth:** Targeted privacy and data-flow refresh; not a legal opinion
> or a full DPIA.
> **References:** Regulation (EU) 2016/679, official text:
> https://eur-lex.europa.eu/eli/reg/2016/679/oj/eng. CNIL practice guide
> "GDPR - Security of personal data", version 2024:
> https://www.cnil.fr/sites/default/files/2024-03/cnil_guide_securite_personnelle_ven_0.pdf.

## Executive Summary

QoreDB remains strongly aligned with a local-first privacy model. Database
connections, queries, results, and most working data stay on the user's machine.
Credentials are separated from connection metadata and stored in the vault or OS
keyring. Persistent diagnostics are disabled by default, and PostHog telemetry
requires explicit opt-in.

The compliance posture is **good**, but it should no longer be described as
"high" without qualification. QoreDB persists several local work states that may
contain sensitive data, especially crash recovery drafts, notebook drafts,
sandbox backups, and workspace state. The main GDPR risk is not server-side
exfiltration by QoreDB; it is local data lifecycle governance: what is stored,
for how long, where it is visible, and how users can delete it.

## Review Evidence

Lightweight commands used during the 2026-07-08 review:

- `rg "localStorage|sessionStorage" src src-tauri`
- `rg "shouldStoreHistory|shouldStoreErrorLogs|analytics|PostHog|opt|consent|crashRecovery" src/components src/lib src/providers`
- `rg "keyring|vault|Sensitive<|redacted_field" src-tauri/crates/qore-service/src/vault src-tauri/crates/qore-core/src src-tauri/src/commands/vault.rs`
- `rg "time_travel|redacted_columns|sandbox_backup" src-tauri/src src/lib/sandbox src/components/Sandbox`

No compiling test suite was run during this pass.

## 1. Data Map

| Surface | Data | Local / third party | Retention / control | GDPR status |
| --- | --- | --- | --- | --- |
| Saved connections | Connection metadata, environment, SSH/proxy options | Local vault metadata plus vault/keyring for secrets | Deleted through connection management | Good |
| DB/SSH/proxy secrets | Passwords, passphrases, proxy password | OS keyring by default; encrypted-file fallback when `QORE_VAULT_KEY` is set | Vault lock, optional master password, deletion per connection | Good |
| Query history | Executed queries, driver, session, redacted errors | `localStorage` only when diagnostics are enabled | Disabled by default; `clearHistory()` when disabled | Good |
| Frontend error logs | Redacted error messages and details | `localStorage` only when diagnostics are enabled | Disabled by default; `clearErrorLogs()` when disabled | Good |
| Backend audit/profiling | Redacted queries and slow queries, fingerprint, metadata | Local backend files | Redaction before persistence | Good |
| Crash recovery | Tabs, active connection, query drafts when enabled | `localStorage` | 24-hour TTL by default; query drafts enabled by default | Needs attention |
| Notebook drafts | Per-tab notebook content (`qnb_draft_<tabId>`) | `localStorage` | No global TTL identified during this review | Needs attention |
| Sandbox backup | Sandbox state and backup (`qoredb_sandbox_backup`) | `localStorage` | Managed by sandbox logic; retention needs documentation | Needs attention |
| Time Travel | Local changelog of mutations | Local backend files | 30-day retention by default; sensitive columns redacted by name | Good with caveat |
| AI BYOK | Preferred provider, sample-row preference, requests to chosen provider | Local preference plus user-selected provider | Sample rows are opt-in; sensitive schema/columns are redacted | Good with caveat |
| PostHog telemetry | Anonymous usage events | PostHog, EU host by default | Explicit opt-in; reset/opt-out available | Good |
| Auto-updater | GitHub releases request | GitHub/Microsoft | Startup preference; IP is visible to the third party | Document |
| Project/config exports | Connections without credentials, optional query library | User-selected file | Query redaction option for project/library exports | Good with caveat |

## 2. Strengths

### 2.1 Local-First Minimization

- User databases do not transit through a QoreDB-managed server.
- Telemetry is only loaded and sent when `qoredb_analytics_enabled === "true"`.
- Persistent diagnostics (`qoredb_query_history`, `qoredb_error_logs`) are
  disabled by default in `diagnosticsSettings.ts`.
- Project exports set `credentialsIncluded: false`.

### 2.2 Secret Confidentiality

- Connection secrets are wrapped in `StoredCredentials` with
  `Sensitive<String>`.
- The default provider uses the OS keyring through `keyring::Entry`.
- The headless/container fallback encrypts `vault.enc` when `QORE_VAULT_KEY` is
  set.
- Explicit credential reads require fresh authentication when the vault is
  protected by a master password.

### 2.3 Redaction

- The backend audit log and profiling store call `redact_query` before
  persistence.
- Query redaction covers SQL, MongoDB, and Redis forms documented in the
  security audit.
- AI schema context redacts sensitive column names, and sample rows are opt-in.
- Time Travel redacts columns whose names match the shared sensitive-token list
  in `src-tauri/src/redaction.rs`.

## 3. Active Risks

### Medium: Recovery and Draft Persistence Are More Permissive Than Diagnostics

**Locations:** `src/lib/diagnostics/crashRecovery.ts`,
`src/lib/diagnostics/crashRecoverySettings.ts`,
`src/providers/SessionProvider.tsx`, `src/lib/notebook/notebookIO.ts`.

Crash recovery saves query drafts by default (`saveQueryDrafts: true`, 24-hour
TTL). Notebooks have their own local cache, `qnb_draft_<tabId>`, with no global
TTL identified during this review.

**Impact:** queries or notes containing personal data can remain on local disk
even when the user believes query history has been disabled.

**Recommendation:** add a "Local data lifecycle" settings panel that lists
local data stores and provides explicit purge actions for crash recovery,
notebooks, sandbox backups, history, and error logs. Consider
`saveQueryDrafts: false` by default for production connections.

### Medium: Local Retention Inventory Is Incomplete

**Locations:** multiple `localStorage` keys, `src/lib/sandbox/sandboxStore.ts`,
`src/lib/tableInsights.ts`, `src/lib/query/queryLibrary.ts`.

Retention is clear for some paths (crash recovery TTL, Time Travel 30 days,
diagnostics disabled by default) but not for every local key. UI state is
generally low sensitivity, but sandbox backups, query libraries, notebooks, and
workspace-local state may contain personal data.

**Recommendation:** maintain a persistent table of local keys and files with
category, sensitivity, retention, purge path, and user-facing purpose.

### Low: Diagnostics Defaults Are Ambiguous in the UI

**Locations:** `src/lib/diagnostics/diagnosticsSettings.ts`,
`src/components/Settings/sections/DataSection.tsx`.

Effective diagnostic storage defaults to `false`, but `DataSection` compares the
current state against `DEFAULTS` set to `true`. This does not force persistence,
but it can make the "modified" state counterintuitive.

**Recommendation:** align the UI defaults with the effective storage defaults,
or rename the constant to "recommended defaults".

### Low: Optional Third Parties Should Stay Visible

**Locations:** `src-tauri/tauri.conf.json`,
`src/components/Onboarding/AnalyticsService.ts`,
`src/providers/SessionProvider.tsx`.

PostHog is opt-in and configured without autocapture or session recording. The
updater queries GitHub releases if the startup preference allows it. These flows
do not transmit query data, but they do contact third parties.

**Recommendation:** clearly document optional third parties: PostHog when
analytics is enabled, GitHub for updates, AI providers when the user enables
BYOK AI, and user-configured share providers.

## 4. Prioritized Recommendations

1. Add a "Local data" view with unified purge controls for recovery, notebooks,
   sandbox backups, history, error logs, cache, and query library data.
2. Make crash recovery explicit: show TTL, `saveQueryDrafts`, and the impact on
   sensitive queries.
3. Add a persistent documentation matrix for `localStorage` keys and local files.
4. Align diagnostics UI defaults with the effective runtime defaults.
5. Document all optional third parties: PostHog, GitHub updater, AI providers,
   and user-configured share providers.
6. Review export flows so query redaction options are visible and enabled by
   default wherever query text can leave the app.

## Conclusion

QoreDB has a good GDPR baseline for a local-first desktop application:
minimization, secrets in the vault/keyring, opt-in telemetry, persistent
diagnostics disabled by default, and backend log redaction. The next important
improvement is not another cryptographic mechanism; it is better governance of
local data lifecycle: what is stored, why, for how long, and how users delete it.
