# QoreDB audits

This directory contains security, privacy, compliance, and driver-focused
self-assessments for QoreDB. The documents are living audits: they preserve
historical findings, but each one should also state when it was last reviewed
and which code baseline was checked.

## Current review snapshot

> **Review date:** 2026-07-08
> **Code baseline:** `c802bae783865b0d8d71d27aad2bf0620cd3c1e3`
> **Review type:** targeted refresh of critical audits; not a full penetration test or formal compliance attestation.

| Audit | Original date | Last reviewed | Status | Next action |
| --- | --- | --- | --- | --- |
| [SECURITY_AUDIT.md](SECURITY_AUDIT.md) | 2026-04-05 | 2026-07-08 | Reviewed, targeted refresh | Follow up on raw crash-recovery and notebook-draft persistence. |
| [PLUGIN_CAPABILITY_CHECKS.md](PLUGIN_CAPABILITY_CHECKS.md) | 2026-05-24 | 2026-07-08 | Reviewed, spot-checked | Convert the manual host-function checklist into a small automated regression test if feasible. |
| [GDPR_AUDIT.md](GDPR_AUDIT.md) | 2025-02-21 | 2026-07-08 | Reviewed, targeted refresh | Add a user-facing local-data lifecycle view and purge controls. |
| [OWASP_ALIGNMENT.md](OWASP_ALIGNMENT.md) | 2026-03-23 | 2026-07-08 | Reviewed, targeted refresh | Monitor remaining dependency-health warnings and add export/governance regression tests. |
| [SOC2_ALIGNMENT.md](SOC2_ALIGNMENT.md) | 2026-03-23 | 2026-07-08 | Reviewed, targeted refresh | Finish local data lifecycle controls and keep dependency-audit evidence current. |
| [CLAIMS_VS_CODE.md](CLAIMS_VS_CODE.md) | 2026-07-17 | 2026-07-17 | Needs update | Validate SQL Server TLS certificates (A2), then rewrite the Sandbox and Visual Diff discovery-panel strings. Extend the pass to performance claims and the remaining drivers. |
| [BUILD_WEIGHT_AUDIT.md](BUILD_WEIGHT_AUDIT.md) | 2026-07-26 | 2026-07-26 | Reviewed, targeted optimization applied | Validate static DuckDB packaging on the full release matrix; evaluate Arrow version alignment separately. |

`PLUGIN_TRUST.md` was not present in `doc/audits` during this review pass, so
it is not indexed here. If it is restored, add it to this table and give it the
same review metadata block as the other audits.

## Review statuses

- **Reviewed, spot-checked**: metadata and narrow evidence checks were updated,
  but the document was not fully re-audited end to end.
- **Needs targeted refresh**: the main conclusions probably still stand, but a
  specific surface has changed or needs sharper evidence.
- **Needs revalidation**: the document depends on implementation details that
  may have drifted since the original audit.
- **Needs update**: the audit references stale findings, stale standards, or
  contradictory status text.
- **Needs evidence refresh**: the framework mapping is useful, but the control
  rows need current file/test/command evidence.

## Standard metadata block

New and updated audits should start with:

```md
> **Status:** Reviewed / Needs update / Needs revalidation
> **Original date:** YYYY-MM-DD
> **Last reviewed:** YYYY-MM-DD
> **Code baseline:** `<git commit>`
> **Review depth:** Freshness pass / targeted revalidation / full re-audit
> **Scope:** ...
```

## Baseline checks for the next full pass

Run the relevant subset before upgrading any document from freshness review to
full re-audit:

```bash
git rev-parse HEAD
pnpm audit
cargo audit
cargo deny check
cargo test
pnpm test
rg "dangerouslySetInnerHTML|eval\(|new Function" src src-tauri
rg "localStorage|sessionStorage" src src-tauri
rg "is_read_only|has_capability|redact_query|validate_share_url" src-tauri/src src-tauri/crates
```

## External references to re-check

- OWASP Top 10: https://owasp.org/www-project-top-ten/
- AICPA Trust Services Criteria: https://www.aicpa-cima.com/resources/download/2017-trust-services-criteria-with-revised-points-of-focus-2022
- GDPR official text: https://eur-lex.europa.eu/eli/reg/2016/679/oj/eng
