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
  - No telemetry is collected; the app ships no analytics SDK.
  - Crash reports are written to disk only. Sharing one opens a prefilled GitHub
    issue in the user's browser, after credentials and home paths are scrubbed.
  - Every log egress path is scrubbed, not just crash reports: the diagnostic
    log export runs through the same filter, since an export exists to be sent
    to someone. Files on disk stay raw for the user's own debugging.
  - CSP restricts network origins for the webview itself.
  - **Current limitation**: Custom share providers currently accept both `http` and `https`; deployments should prefer HTTPS-only endpoints.

### 7. Agent Surfaces (MCP Server and CLI)

- **Threat**: An AI agent, or any local process able to launch `qore-mcp` or `qore`, reads data it should not, or is steered by hostile data into damaging queries.
- **Mitigation**:
  - **Opt-in per connection**: nothing is visible to agents until the user switches the connection on under Settings > AI agents or in its connection form. The flag is stored with the connection and checked before any secret is read, even when the caller already knows the connection id.
  - **Read-only by construction**: agent sessions are opened with `read_only` forced on, then every statement goes through the same preflight as the editor (SQL, Mongo, Redis and search classification), the safety policy (row cap, duration, rate limit) and the audit log with source `mcp` or `cli`.
  - **Local transport only**: the MCP server speaks stdio to the client that spawned it; it opens no network listener.
  - **Bounded lifetime**: sessions idle for ten minutes are closed; the safety policy and the vault are reread on every call so a change in the app applies to the next agent query.
  - **Store scoping**: the server reads the default vault, or the `.qoredb` workspace it was pointed at or detected from its working directory, never both.
  - **Federation (Pro)**: `run_federated_query` checks the license on every call and only joins exposed connections. The join runs in an in-memory DuckDB whose external access (files, URLs, extensions) is switched off and locked once the source rows are loaded, so a query cannot read `read_csv('/etc/passwd')`. The call is audited under the strictest environment of its sources and truncated to the policy row cap.
  - **Saved queries**: `run_saved_query` reads the library of the selected workspace only. Declared variables are substituted as literals (numbers and dates validated, text quoted) and the statement goes through the same read-only preflight as `run_query`.
- **Current limitation**:
  - Source fetches of a federated query run the planner's generated `SELECT` directly on each session: the audit log holds one entry for the federated query, not one per source.
  - The app's master-password lock is not shared: the server reads secrets from the OS keyring directly, so a vault locked in the app stays readable by an agent as long as the keyring is unlocked. The exposure flag is the effective gate.
  - Query results are returned to the agent and may be sent to a remote model by the client; column masking is not applied yet (planned, see the v0.1.39 scope).
