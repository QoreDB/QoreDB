# SOC 2 Trust Services Criteria Self-Assessment

> **Status:** Reviewed, targeted refresh
> **Original date:** 2026-03-23
> **Last reviewed:** 2026-07-08
> **Code baseline:** `c802bae783865b0d8d71d27aad2bf0620cd3c1e3`
> **Review depth:** Evidence-backed self-assessment refresh; not a SOC 2 Type I or Type II audit.
> **Project:** QoreDB, a local-first desktop database client built with Tauri 2, Rust, and React.
> **Reference:** AICPA 2017 Trust Services Criteria with revised 2022 points of focus, https://www.aicpa-cima.com/resources/download/2017-trust-services-criteria-with-revised-points-of-focus-2022

## Scope and Limits

This document is an internal alignment assessment. It does not assert SOC 2
certification, auditor attestation, or operating effectiveness over a review
period. QoreDB is a desktop product, not a hosted multi-tenant SaaS service, so
several criteria are interpreted through product controls, release controls,
local data handling, and support diagnostics rather than through corporate
operations, HR controls, physical datacenter controls, or centralized security
operations.

The AICPA Trust Services Criteria cover controls over security, availability,
processing integrity, confidentiality, and privacy. This refresh adds concrete
evidence and keeps unresolved items visible instead of marking the framework
mapping as fully satisfied.

## Current Evidence Snapshot

| Evidence | Result |
| --- | --- |
| `git rev-parse HEAD` | `c802bae783865b0d8d71d27aad2bf0620cd3c1e3` |
| `pnpm audit --audit-level=high` | Passed on 2026-07-08: no known npm vulnerabilities found. |
| `cargo audit` | Passed on 2026-07-08 after updating `crossbeam-epoch` from `0.9.18` to `0.9.20`; allowed warnings remain for unmaintained/unsound transitive crates. |
| `cargo deny check advisories licenses sources` | Passed on 2026-07-08 after the same lockfile update; warnings remain for unmatched license allowances and a stale ignored advisory entry. |
| `cargo deny check bans` | Passed on 2026-07-08; duplicate crate-version warnings remain. |
| `.github/workflows/ci.yml` | CI includes npm and Rust advisory checks plus `cargo deny check`. |
| `.github/workflows/release.yml` | Release workflow builds artifacts, generates CycloneDX SBOMs, and publishes SHA-256 checksums. |
| `src-tauri/tauri.conf.json` | CSP configured; updater enabled with a public key. |
| `src-tauri/capabilities/default.json` | Tauri filesystem scope limits app read/write paths and denies common sensitive paths. |
| `src-tauri/deny.toml` | Unknown registries and git sources are denied; advisory exceptions require reasons. |

## Overall Assessment

| Area | Current status | Main observation |
| --- | --- | --- |
| Security | Mostly aligned | Product security controls are strong; the live Rust advisory found during the refresh was fixed, while dependency-health warnings remain. |
| Availability | Partially aligned | Local-first operation reduces cloud dependency; recovery and updater workflows exist, but no formal SLO/DR program is in scope. |
| Processing integrity | Mostly aligned | Query classification, typed errors, and export paths exist; export and malformed-state regression coverage should be strengthened. |
| Confidentiality | Mostly aligned | Credentials and plugin secrets are protected with keyring/vault controls; local non-secret sensitive data lifecycle remains incomplete. |
| Privacy | Partially aligned | Telemetry is intended to be opt-in and local-first by design; user-facing inventory and purge controls remain a gap. |

## CC1 - Control Environment

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC1.1 Integrity and ethical values | Open-source product with auditable code and documented security expectations. | `LICENSE`, `CONTRIBUTING.md`, `doc/security/THREAT_MODEL.md`, audit docs. | Mostly aligned |
| CC1.2 Board or governance oversight | Project-level governance exists through documented release, security, and contribution practices, but not through a formal board structure. | `CONTRIBUTING.md`, release workflows, audit directory. | Partially aligned |
| CC1.3 Structure and responsibility | Product boundaries are separated across frontend, Rust backend, service crates, drivers, plugin runtime, and release workflows. | `src/`, `src-tauri/src/`, `src-tauri/crates/`, `.github/workflows/`. | Mostly aligned |
| CC1.4 Competence | Security-sensitive code has dedicated audits and targeted tests in Rust modules. | `doc/audits/*`, tests in vault, redaction, plugin runtime, and SQL safety modules. | Mostly aligned |

**Gap:** This is project governance, not an enterprise control environment.
Formal HR, vendor management, board oversight, and independent audit evidence
are out of scope unless QoreDB pursues a real SOC 2 engagement.

## CC2 - Communication and Information

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC2.1 Internal information quality | Security and privacy findings are maintained in living audit documents. | `doc/audits/README.md`, refreshed audit docs. | Mostly aligned |
| CC2.2 Internal communication | Engineering expectations and design constraints are documented in repo-level guidance. | `CONTRIBUTING.md`, `doc/rules/DESIGN.md`, security docs. | Mostly aligned |
| CC2.3 External communication | Public-facing security posture is represented through open-source code, release notes, and issue/PR workflows. | GitHub workflows, release workflow, audit docs. | Partially aligned |

**Gap:** There is no formal customer-facing SOC 2 control narrative, incident
communication procedure, or enterprise security questionnaire package.

## CC3 - Risk Assessment

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC3.1 Objectives | Product risks are mapped through security, GDPR, OWASP, plugin, and SOC 2 audits. | `doc/audits/SECURITY_AUDIT.md`, `GDPR_AUDIT.md`, `OWASP_ALIGNMENT.md`, `PLUGIN_CAPABILITY_CHECKS.md`. | Mostly aligned |
| CC3.2 Risk identification | Main risks include credential theft, destructive queries, local data leakage, plugin abuse, and supply-chain compromise. | Threat model and current audit findings. | Mostly aligned |
| CC3.3 Fraud risk | Fraud risk is limited in the product context because QoreDB is not a billing or transaction-processing SaaS system. | Product scope. | Not applicable |
| CC3.4 Significant change | Release and CI workflows provide repeatable checks; audits are refreshed after material drift. | `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `doc/audits/README.md`. | Partially aligned |

**Gap:** Risk assessment is product-focused and not yet a formal recurring
business risk-management process.

## CC4 - Monitoring Activities

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC4.1 Ongoing monitoring | CI includes dependency and policy scans. | `pnpm audit`, `cargo audit`, `cargo deny check` in `.github/workflows/ci.yml`. | Partially aligned |
| CC4.2 Deficiency evaluation | Audits preserve open findings and recommended follow-ups. | A03 supply-chain advisory was found and resolved during refresh; GDPR local-data lifecycle finding remains open. | Mostly aligned |
| CC4.3 Communication of deficiencies | Findings are documented in repo audit files rather than hidden in command output. | `doc/audits/OWASP_ALIGNMENT.md`, this document. | Mostly aligned |

**Gap:** Monitoring is only effective if failing dependency checks block release
or are explicitly risk-accepted. As of 2026-07-08, the blocking Rust advisory
was fixed, but dependency-health warnings still need periodic review.

## CC5 - Control Activities

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC5.1 Control selection | Security controls are selected around local-first operation, vault protection, backend governance, plugin sandboxing, and release integrity. | Threat model and audit docs. | Mostly aligned |
| CC5.2 Technology controls | Backend policy checks, Tauri capability scopes, plugin host-function gates, and vault controls enforce key protections. | `READ_ONLY_BLOCKED` command paths, `has_capability`, `src-tauri/capabilities/default.json`, vault modules. | Mostly aligned |
| CC5.3 Policy deployment | CI and release workflows encode repeatable checks, SBOM generation, and checksum publication. | `.github/workflows/ci.yml`, `.github/workflows/release.yml`. | Partially aligned |

**Gap:** Some controls are documented and implemented but still need targeted
regression tests that prove they cannot be bypassed through direct Tauri command
invocation or plugin host calls.

## CC6 - Logical and Physical Access Controls

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC6.1 Logical access | Credentials and plugin secrets are kept in OS keyring or encrypted vault storage. | `workspace/connection_store.rs`, plugin secrets runtime, vault modules. | Mostly aligned |
| CC6.2 Authentication | Master password uses Argon2id; local API tokens are stored as Argon2 hashes. | `vault/lock.rs`, `api/auth.rs`. | Mostly aligned |
| CC6.3 Authorization | Backend read-only checks and plugin capability gates enforce privileged operations. | Command modules and plugin runtime host functions. | Mostly aligned |
| CC6.4 Physical access | Physical device security is delegated to the user and OS. | Desktop product scope. | Not applicable |
| CC6.5 Credential lifecycle | Credentials can be saved and deleted with keyring cleanup paths. | Workspace and plugin secret management commands. | Mostly aligned |
| CC6.6 External threats | CSP, filesystem deny lists, plugin private-network checks, and dependency scans reduce exposure. | Tauri config, capabilities, plugin runtime, CI. | Partially aligned |
| CC6.7 Data transmission | Database and provider connectivity rely on configured TLS/SSH mechanisms where supported. | Driver and credential modules; server README for vault behavior. | Mostly aligned |
| CC6.8 Data leakage controls | Sensitive wrappers and redaction helpers reduce logging exposure. | `Sensitive<T>`, interceptor redaction tests. | Mostly aligned |

**Gap:** Local non-secret files such as diagnostics, crash recovery, notebook
drafts, and app metadata still need a complete user-facing inventory and purge
model.

## CC7 - System Operations

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC7.1 Detect anomalies | Structured logs, query metadata, and error handling support local diagnosis. | Service logging and interceptor code. | Mostly aligned |
| CC7.2 Monitor components | npm and Rust dependency checks run locally and in CI. | Audit commands and CI workflow. | Partially aligned |
| CC7.3 Evaluate events | Query classification distinguishes safe, mutation, and dangerous operations. | SQL safety code and read-only command checks. | Mostly aligned |
| CC7.4 Incident response | Local app can ship fixes through the updater; logs/diagnostics can support support response. | Tauri updater config, release workflow. | Partially aligned |
| CC7.5 Recovery | Local-first operation and keyring-backed secrets reduce cloud recovery dependency. | Vault and workspace architecture. | Partially aligned |

**Gap:** There is no formal incident-response runbook, central alerting system,
or tested recovery-time objective for enterprise operations.

## CC8 - Change Management

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC8.1 Change management | Source control, CI, dependency checks, release workflows, SBOMs, checksums, and audit refreshes support controlled change. | GitHub workflows, lockfiles, audit directory. | Partially aligned |

**Gap:** A formal change-approval record, segregation of duties, and release
sign-off evidence are not present in this repo-level self-assessment.

## CC9 - Risk Mitigation

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| CC9.1 Vendor and dependency risk | Lockfiles, `cargo deny`, `cargo audit`, `pnpm audit`, and SBOMs identify third-party risk. | `pnpm audit`, `cargo audit`, and `cargo deny` checks passed after the `crossbeam-epoch` lockfile update. | Mostly aligned |
| CC9.2 Risk mitigation | Unknown registries and git sources are denied; advisory exceptions are explicit. | `src-tauri/deny.toml`. | Partially aligned |

**Gap:** The blocking Rust advisory is resolved, but duplicate dependency
versions and RustSec warning-only advisories should be reviewed periodically.

## Availability

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| A1.1 Availability commitments | Core product functionality is local-first and does not require QoreDB-hosted infrastructure. | Product architecture. | Mostly aligned |
| A1.2 Capacity and recovery | Local operation reduces hosted capacity risk; updater supports hotfix distribution. | Tauri updater and release workflow. | Partially aligned |

**Gap:** No formal availability SLA, uptime monitoring, disaster recovery test,
or business-continuity program is in scope for this desktop self-assessment.

## Confidentiality

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| C1.1 Identify confidential information | Credentials, API keys, plugin secrets, query text, and query results are treated as sensitive. | Vault, plugin secrets, redaction, GDPR audit. | Mostly aligned |
| C1.2 Protect confidential information | OS keyring, encrypted vault fallback, `Sensitive<T>`, and log redaction protect high-risk values. | Vault modules and redaction tests. | Mostly aligned |

**Gap:** Confidentiality protection is strongest for secrets. Local operational
data that is not formally a secret can still be sensitive and needs clearer
retention and purge controls.

## Processing Integrity

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| PI1.1 Complete and accurate processing | Query classification, typed drivers, and backend command validation reduce accidental unsafe behavior. | SQL safety and command modules. | Mostly aligned |
| PI1.2 Error detection | Typed errors, query limits, plugin runtime limits, and share URL validation reduce unsafe failure modes. | Service error handling, plugin runtime, share manager. | Mostly aligned |

**Gap:** Add regression coverage for generated HTML export escaping, malformed
local state, and direct command/plugin denial paths.

## Privacy

| Criterion | QoreDB control | Evidence | Status |
| --- | --- | --- | --- |
| P1.1 Notice | GDPR audit documents expected telemetry and local-data notice obligations. | `doc/audits/GDPR_AUDIT.md`. | Partially aligned |
| P1.2 Choice and consent | Telemetry and AI integrations are treated as optional surfaces. | GDPR audit and CSP PostHog visibility. | Partially aligned |
| P1.3 Collection limitation | QoreDB is local-first; no implicit cloud data plane is required for core operation. | Product architecture. | Mostly aligned |
| P1.4 Use, retention, and disposal | Local storage, diagnostics, crash recovery, drafts, and analytics identity still need a complete lifecycle view. | GDPR audit open findings. | Open |
| P1.5 Access and correction | Users can manage local project/workspace files, but no unified personal-data export/purge UX exists. | Product scope and GDPR audit. | Partially aligned |

**Gap:** Privacy alignment is the least complete trust-services area because
local data inventory, retention, and purge UX are not yet cohesive.

## Open Findings

| ID | Finding | Priority | Recommended action |
| --- | --- | --- | --- |
| SOC2-1 | Rust dependency advisory `RUSTSEC-2026-0204` was found and resolved during this refresh by updating `crossbeam-epoch` to `0.9.20`. | Resolved | Keep `cargo audit` and `cargo deny` results in the audit trail. |
| SOC2-2 | Local data lifecycle is incomplete for diagnostics, crash recovery, notebook drafts, and app metadata. | High | Add a user-facing inventory and purge flow. |
| SOC2-3 | Some governance controls lack direct bypass regression tests. | Medium | Add tests for direct Tauri command invocation and plugin host-function denials. |
| SOC2-4 | Broad Tauri opener scope is not fully justified in the audit trail. | Medium | Narrow the scope or document why the export/open workflow needs it. |
| SOC2-5 | No formal SOC 2 operational program exists. | Low | Only pursue if enterprise requirements justify Type I or Type II readiness work. |

## Conclusion

QoreDB is meaningfully aligned with many SOC 2 security and confidentiality
expectations at the product-control level, especially around credentials,
backend governance, plugin sandboxing, release integrity, and local-first
operation. It is not SOC 2 ready in the formal attestation sense. The most
important next actions are finishing local data lifecycle controls, reviewing
dependency-health warnings, and adding regression tests that prove critical
security gates fail closed.
