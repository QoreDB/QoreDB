# OWASP Top 10:2025 Alignment Assessment

> **Status:** Reviewed, targeted refresh
> **Original date:** 2026-03-23
> **Last reviewed:** 2026-07-08
> **Code baseline:** `c802bae783865b0d8d71d27aad2bf0620cd3c1e3`
> **Review depth:** Targeted revalidation and remapping from OWASP Top 10:2021 to OWASP Top 10:2025; not a penetration test.
> **Project:** QoreDB, a local-first desktop database client built with Tauri 2, Rust, and React.
> **Reference:** OWASP Top 10:2025, https://owasp.org/Top10/2025/0x00_2025-Introduction/

## Executive Summary

QoreDB is a desktop application rather than an internet-facing web service, so
this assessment adapts OWASP web-application categories to the Tauri, local
storage, plugin-runtime, database-client, and release-supply-chain surfaces.

The core architecture remains strong: backend Rust commands are the authority
for policy decisions, credentials are stored in the OS keyring or an encrypted
vault fallback, plugin host functions enforce declared capabilities, Tauri CSP
and filesystem scopes are configured, and release workflows publish SBOMs and
SHA-256 checksums.

The main change since the previous 2021-based document is the OWASP 2025
category model. SSRF is now treated under Broken Access Control, supply-chain
risk is elevated to A03, and Exceptional Conditions is a new explicit category.

The current risk posture is not all green, but the blocking dependency advisory
found during this refresh was fixed in the same pass. `crossbeam-epoch` was
updated from `0.9.18` to `0.9.20` in `src-tauri/Cargo.lock`, resolving
`RUSTSEC-2026-0204`. `pnpm audit --audit-level=high`, `cargo audit`,
`cargo deny check advisories licenses sources`, and `cargo deny check bans`
now exit successfully on 2026-07-08. RustSec still reports allowed warnings
for unmaintained/unsound transitive crates, and `cargo deny` reports duplicate
crate versions, so A03 remains a monitored supply-chain area rather than a
blanket "no risk" claim.

## Evidence Checked

| Evidence | Result |
| --- | --- |
| `src-tauri/tauri.conf.json` | CSP restricts scripts to `self`; updater is active and configured with a public key. |
| `src-tauri/capabilities/default.json` | Filesystem scope is limited to app config/data/local data; sensitive home paths and system paths are denied. The opener scope is broader and remains a review item. |
| `.github/workflows/ci.yml` | Runs `pnpm audit --audit-level=high`, `cargo audit`, and `cargo deny check`. |
| `.github/workflows/release.yml` | Generates Rust and frontend CycloneDX SBOMs and SHA-256 checksums for release assets. |
| `src-tauri/deny.toml` | Denies unknown registries and unknown git dependencies; tracks advisory exceptions with reasons. |
| `rg "dangerouslySetInnerHTML\|eval\\(\|new Function\|innerHTML\|document.write" src src-tauri` | No app `eval`, `new Function`, `document.write`, or `dangerouslySetInnerHTML` hits. Only the generated HTML export writer uses `innerHTML`, with escaping helpers. |
| `pnpm audit --audit-level=high` | Passed: no known npm vulnerabilities found. |
| `cargo audit` | Passed after updating `crossbeam-epoch` to `0.9.20`; 21 allowed warnings remain for unmaintained/unsound transitive crates. |
| `cargo deny check advisories licenses sources` | Passed after the same lockfile update; warnings remain for unmatched license allowances and a stale ignored advisory entry. |
| `cargo deny check bans` | Passed; duplicate crate-version warnings remain. |

## A01:2025 - Broken Access Control

**Applicability:** High. QoreDB exposes privileged local backend commands,
database mutation operations, workspace metadata, plugin host functions, and
network destinations selected by the user or by plugins.

| Control | Evidence | Status |
| --- | --- | --- |
| Backend-side policy authority | `READ_ONLY_BLOCKED` checks are present across query, import, maintenance, routines, sequences, and triggers commands. | Mitigated |
| Limits bypass is gated backend-side | `execute_query` treats `bypass_limits=true` as a privileged request and rejects it without the required license tier. | Mitigated |
| Plugin capabilities are enforced in host functions | `has_capability` gates log, notify, storage, HTTP, filesystem, secrets, and query-read host functions. | Mitigated |
| Plugin private-network access is constrained | `is_private_destination` detects loopback, RFC1918, link-local, CGNAT, unspecified, and IPv6 private destinations; private access requires manifest opt-in. | Mitigated |
| Tauri filesystem access is scoped | App config/data/local data are allowed; common sensitive paths are denied. | Mitigated |
| Opener access | `opener:allow-open-path` allows broad user locations including home, documents, downloads, and desktop. This may be acceptable for a desktop export/open workflow, but it is broader than the filesystem scope and should stay explicitly justified. | Review item |

**Conclusion:** Mostly mitigated for the desktop threat model. Keep regression
tests focused on backend gates, because frontend/UI-only restrictions are not
sufficient in a Tauri application.

## A02:2025 - Security Misconfiguration

**Applicability:** High. Misconfiguration is a central risk for desktop
capabilities, CSP, filesystem access, telemetry, updater behavior, and plugin
runtime controls.

| Control | Evidence | Status |
| --- | --- | --- |
| CSP | `script-src 'self'`; image and font sources are explicit; `connect-src` allows IPC/Tauri localhost and PostHog endpoints. | Mitigated |
| Inline styles | `style-src 'self' 'unsafe-inline'` is enabled. This is common in React/Tauri stacks but weaker than nonce/hash-based style policies. | Accepted caveat |
| Filesystem deny list | Sensitive home files and system directories are denied in Tauri fs scope. | Mitigated |
| Telemetry boundary | PostHog endpoints are visible in CSP; GDPR audit documents telemetry consent expectations. | Needs consent verification |
| Plugin runtime limits | Wasm runtime uses fuel and store limits. | Mitigated |

**Conclusion:** Configuration is materially better than the original audit
baseline, but the broad opener scope and inline styles should remain known
desktop tradeoffs rather than disappearing from the audit.

## A03:2025 - Software Supply Chain Failures

**Applicability:** High. OWASP 2025 expands this category beyond vulnerable
components to dependency, build, and distribution trust.

| Control | Evidence | Status |
| --- | --- | --- |
| npm advisory scanning | `pnpm audit --audit-level=high` passed on 2026-07-08. | Mitigated |
| Rust advisory scanning | `cargo audit` passed after updating `crossbeam-epoch` from `0.9.18` to `0.9.20`. | Mitigated with warnings |
| Policy scanning | `cargo deny check advisories licenses sources` and `cargo deny check bans` passed after the update. | Mitigated with warnings |
| Lockfiles | `pnpm-lock.yaml` and `src-tauri/Cargo.lock` are present. | Mitigated |
| Registry/source policy | `src-tauri/deny.toml` denies unknown registries and unknown git sources. | Mitigated |
| SBOM | Release workflow generates CycloneDX SBOMs for Rust and frontend dependencies. | Mitigated |
| Release integrity | Release workflow generates SHA-256 checksums; updater is configured with a public key. | Mitigated |

**Finding A03-1, resolved during refresh:** `crossbeam-epoch 0.9.18` was
affected by `RUSTSEC-2026-0204`. It was upgraded to `0.9.20` with
`cargo update -p crossbeam-epoch --precise 0.9.20`.

**Monitoring item A03-2:** `cargo audit` still prints allowed warnings for
unmaintained/unsound transitive crates, and `cargo deny check bans` prints
duplicate-version warnings. These are not currently failing checks, but they
should be reviewed periodically because they represent dependency health debt.

**Conclusion:** Supply-chain controls are present and currently pass after the
lockfile update. This category should remain high-attention because the warning
set is non-empty and the Rust desktop stack includes large transitive trees.

## A04:2025 - Cryptographic Failures

**Applicability:** High. QoreDB stores database credentials, API keys, plugin
secrets, license material, local workspace metadata, and optional diagnostics.

| Control | Evidence | Status |
| --- | --- | --- |
| Credential storage | Workspace connection credentials and plugin secrets are stored in the OS keyring. | Mitigated |
| Headless fallback | Server README documents encrypted-file vault fallback using XChaCha20Poly1305 and Argon2id when `QORE_VAULT_KEY` is set. | Mitigated |
| Master password | Vault lock uses hardened Argon2id parameters and tracks failed attempts. | Mitigated |
| Sensitive values in logs | `Sensitive<T>` wrappers and query redaction helpers exist in service code. | Mitigated |
| Local non-secret persistence | Crash recovery, notebook drafts, and app-local metadata may contain operational data and are not all protected like vault secrets. | Open privacy/security caveat |

**Conclusion:** Secrets are handled with appropriate cryptographic primitives
for the desktop model. The remaining risk is not credential encryption; it is
inventorying and purging local non-secret data that may still be sensitive.

## A05:2025 - Injection

**Applicability:** High. QoreDB intentionally executes user-supplied database
queries and also renders query results, exports data, and runs plugin code.

| Control | Evidence | Status |
| --- | --- | --- |
| SQL safety classification | SQL safety logic uses parser-based classification in `qore-sql`. | Mitigated |
| Dangerous operation blocking | Backend commands block read-only mutations and dangerous operations in relevant paths. | Mitigated |
| Query redaction | Driver-aware redaction tests cover SQL, MongoDB-like strings, and Redis secret-bearing commands. | Mitigated |
| Frontend script sinks | No app `eval`, `new Function`, `document.write`, or `dangerouslySetInnerHTML` hits were found. | Mitigated |
| Exported HTML writer | `src-tauri/src/export/writers/html.rs` uses `innerHTML` in the generated artifact. The writer also defines escaping helpers; keep regression tests around generated HTML escaping. | Review item |

**Conclusion:** Injection is mitigated for application code and database
command governance, with one export-specific review item because generated HTML
uses dynamic HTML rendering by design.

## A06:2025 - Insecure Design

**Applicability:** High. The design must account for local-first operation,
highly privileged database credentials, plugins, optional AI/telemetry, and
enterprise release expectations.

| Control | Evidence | Status |
| --- | --- | --- |
| Threat modeling | `doc/security/THREAT_MODEL.md` exists and is referenced by other audits. | Mitigated |
| Local-first privacy boundary | GDPR audit documents the local-data model and retention gaps. | Partially mitigated |
| Plugin trust model | Plugin capability audit covers host-function gating, private-network handling, secrets, and integrity. | Mitigated |
| Safety-by-default for mutations | Read-only blocking and dangerous-query controls exist in backend paths. | Mitigated |
| Local data lifecycle | A unified inventory/purge UX for local diagnostics, drafts, and recovery data is still missing. | Open |

**Conclusion:** The architecture is intentionally defensive, but local data
lifecycle design needs to be completed so privacy and incident-response controls
are discoverable by users rather than only implicit in storage locations.

## A07:2025 - Authentication Failures

**Applicability:** Medium. QoreDB is primarily a single-user desktop
application, but it still authenticates to vaults, database servers, optional
local API endpoints, and enterprise workflows.

| Control | Evidence | Status |
| --- | --- | --- |
| Vault authentication | Master password hashing uses Argon2id; fresh-authentication checks protect sensitive vault operations. | Mitigated |
| OS authentication boundary | OS keyring integration delegates secret access to platform credential storage. | Mitigated |
| API tokens | Local API token storage uses Argon2 hashes and constant-time verification. | Mitigated |
| Multi-user identity | QoreDB does not provide a cloud identity plane; this is by design for local-first desktop use. | Not applicable |

**Conclusion:** Authentication controls are appropriate for a local desktop
product. Do not market this as multi-user enterprise identity without adding a
separate identity and authorization model.

## A08:2025 - Software or Data Integrity Failures

**Applicability:** High. This includes updater integrity, release artifacts,
plugin Wasm integrity, and persisted workspace data.

| Control | Evidence | Status |
| --- | --- | --- |
| Updater integrity | Tauri updater is active and configured with a public key. | Mitigated |
| Release integrity | Release workflow produces SHA-256 checksums and SBOM artifacts. | Mitigated |
| Platform signing | Windows MSIX signing is present when signing secrets are available; release workflows include signing steps. | Mitigated with CI-secret dependency |
| Plugin Wasm integrity | Plugin manager verifies expected digests before loading Wasm. | Mitigated |
| Plugin trust semantics | A matching digest proves integrity of a known artifact, not that the plugin author or registry is trusted. | Review item |

**Conclusion:** Integrity controls are solid, but documentation should keep
separating artifact integrity from author trust and marketplace governance.

## A09:2025 - Security Logging and Alerting Failures

**Applicability:** Medium. A desktop app cannot rely on centralized monitoring
by default, but it must expose enough local evidence for troubleshooting and
incident response.

| Control | Evidence | Status |
| --- | --- | --- |
| Structured logs | Rust service code uses structured logging and typed errors. | Mitigated |
| Query redaction | Interceptor redaction prevents obvious credential leakage in logged queries. | Mitigated |
| Exportable diagnostics | Existing audits reference log export and diagnostics flows. | Mitigated with privacy caveat |
| Alerting | There is no central alerting plane, and that is expected for a local desktop app. | Not applicable |
| Retention transparency | GDPR audit keeps local retention and purge controls open. | Open |

**Conclusion:** Logging is useful for support and local incident review.
Privacy-safe retention and purge controls should be the next improvement rather
than adding centralized alerting by default.

## A10:2025 - Mishandling of Exceptional Conditions

**Applicability:** High. OWASP 2025 adds this category for failing open,
improper error handling, abnormal states, parsing failures, and logical
edge-case behavior.

| Control | Evidence | Status |
| --- | --- | --- |
| Vault failure behavior | Keyring and encrypted-file vault errors are mapped into typed credential errors. | Mitigated |
| Plugin runtime failure behavior | Host functions return explicit denials and runtime limits are configured. | Mitigated |
| Query limits | Even privileged `bypass_limits` keeps an absolute timeout cap. | Mitigated |
| Share URL validation | Share manager validates provider config and outbound share URLs. | Mitigated |
| Export rendering edge cases | Generated HTML uses escaping helpers but should keep explicit regression tests for malformed and hostile cell values. | Review item |
| Local recovery parse failures | Crash recovery and draft persistence should fail closed and expose purge controls. | Open |

**Conclusion:** Exceptional-condition handling is generally defensive, but this
category should drive more regression tests around malformed local state,
generated exports, plugin denial paths, and partial failures.

## Summary

| OWASP 2025 category | Current status | Main reason |
| --- | --- | --- |
| A01 Broken Access Control | Mostly mitigated | Backend policy checks, plugin capability gates, filesystem scoping; opener scope remains broad. |
| A02 Security Misconfiguration | Mostly mitigated | CSP and Tauri scopes are configured; inline styles and opener paths are accepted caveats. |
| A03 Software Supply Chain Failures | Mostly mitigated | Blocking advisory fixed; audit/deny checks pass, with remaining dependency-health warnings. |
| A04 Cryptographic Failures | Mostly mitigated | OS keyring, encrypted vault fallback, Argon2id, sensitive-value wrappers; non-secret local data remains a caveat. |
| A05 Injection | Mostly mitigated | Parser-based SQL safety, backend blocking, no app script sinks; exported HTML needs regression coverage. |
| A06 Insecure Design | Partially mitigated | Defensive architecture exists; local data lifecycle UX remains unfinished. |
| A07 Authentication Failures | Mitigated for desktop scope | Argon2id vault lock, OS keyring, local API token hashes. |
| A08 Software or Data Integrity Failures | Mostly mitigated | Updater key, release checksums, SBOMs, plugin digest verification. |
| A09 Security Logging and Alerting Failures | Partially mitigated | Local logging and redaction exist; retention and user-facing purge controls remain open. |
| A10 Mishandling of Exceptional Conditions | Partially mitigated | Many failure paths are typed/bounded; malformed local state and export edge cases need regression focus. |

## Recommended Follow-Ups

1. Periodically review the remaining `cargo audit` warnings and duplicate crate
   versions reported by `cargo deny check bans`.
2. Add regression tests for generated HTML export escaping and malformed cell
   values.
3. Add regression tests for backend governance bypass attempts, especially
   `bypass_limits`, read-only mode, and plugin capability denial paths.
4. Tighten or document the broad Tauri opener scope for `$HOME`, documents,
   downloads, and desktop paths.
5. Build a user-facing local data lifecycle view covering diagnostics,
   crash-recovery files, notebook drafts, analytics identity, and purge actions.
6. Keep supply-chain evidence in this audit tied to actual command output, not
   only CI workflow intent.
