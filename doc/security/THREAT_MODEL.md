# Threat Model

## Overview

QoreDB is a desktop application (Tauri/Rust) that connects to user databases. The threat model focuses on **protecting credentials**, **preventing accidental misuse**, and **making local persistence behavior explicit**.

## Assets

1.  **Database Credentials**: Usernames, passwords, SSH keys.
2.  **Database Data**: Tables, rows, schema.
3.  **Connection Metadata**: Hostnames, ports, user settings.
4.  **Local App State**: Query history, crash recovery drafts, audit logs, profiling data, share settings.

## Threats & Mitigations

### 1. Local Credential Theft

- **Threat**: Malware on the user's machine stealing saved passwords.
- **Mitigation**:
  - Credentials are stored in the OS Keychain (via `keyring` crate), not plain text files.
  - Access requires OS-level authentication (e.g. TouchID/Password on macOS).
  - Internal memory uses `Sensitive<String>` to redact passwords in logs/debug output.

### 2. Accidental Data Destruction

- **Threat**: User running `DROP TABLE users` on production instead of staging.
- **Mitigation**:
  - **Environment classification**: Connections marked as `Production` or `Development`.
  - **Read-Only Mode**: Enforced on the main query and mutation paths.
  - **Dangerous Query Blocking**: `DELETE` / `UPDATE` without `WHERE` are blocked or require explicit confirmation in production.
  - **Current limitation**: Read-only and governance protections are not yet applied uniformly to every specialized command path (for example some create/drop helpers and some browser endpoints must still be hardened individually).

### 3. Supply Chain Attacks

- **Threat**: Malicious dependency introducing a backdoor.
- **Mitigation**:
  - Minimal dependency tree.
  - Open Source (users can audit requirements).
  - SBOM CycloneDX publié avec chaque release.
  - `cargo-deny` : advisories, licences et registries vérifiés dans le CI.
  - Builds signés avec checksums SHA-256.

### 4. Data Leaks via Logs

- **Threat**: Application logs containing connection strings or query results.
- **Mitigation**:
  - Structured logging (`tracing`) with redaction.
  - Query results are NOT logged by default.
  - Logs are stored locally in user's home directory (`~/.qoredb/logs`).
  - **Current limitation**: The interceptor audit/profiling pipeline currently stores raw query text locally for audit entries and slow-query samples.

### 5. Sensitive Data Persisted in Local UI State

- **Threat**: Queries containing credentials, tokens, or sensitive business data remain on disk after the app is closed.
- **Mitigation**:
  - Query history and frontend error logs are opt-in and redacted before persistence.
  - Saved connection secrets are not written to disk; only metadata is stored locally.
  - **Current limitation**: Crash recovery stores raw query drafts in `localStorage` so the editor can be restored after an unexpected exit.

### 6. Outbound Data Transfer

- **Threat**: Shared exports or AI/network integrations sending data to unintended or weakly protected endpoints.
- **Mitigation**:
  - Share-provider tokens are stored in the OS keyring.
  - Analytics is opt-in.
  - CSP restricts network origins for the webview itself.
  - **Current limitation**: Custom share providers currently accept both `http` and `https`; deployments should prefer HTTPS-only endpoints.
