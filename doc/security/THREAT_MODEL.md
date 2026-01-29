# Threat Model

## Overview

QoreDB is a desktop application (Tauri/Rust) that connects to user databases. The threat model focuses on **protecting credentials** and **preventing accidental misuse**.

## Assets

1.  **Database Credentials**: Usernames, passwords, SSH keys.
2.  **Database Data**: Tables, rows, schema.
3.  **Connection Metadata**: Hostnames, ports, user settings.

## Threats & Mitigations

### 1. Local Credential Theft
*   **Threat**: Malware on the user's machine stealing saved passwords.
*   **Mitigation**:
    *   Credentials are stored in the OS Keychain (via `keyring` crate), not plain text files.
    *   Access requires OS-level authentication (e.g. TouchID/Password on macOS).
    *   Internal memory uses `Sensitive<String>` to redact passwords in logs/debug output.

### 2. Accidental Data Destruction
*   **Threat**: User running `DROP TABLE users` on production instead of staging.
*   **Mitigation**:
    *   **Environment classification**: Connections marked as `Production` or `Development`.
    *   **Read-Only Mode**: Enforced by backend logic, blocking mutation queries.
    *   **Dangerous Query Blocking**: `DELETE` / `UPDATE` without `WHERE` are blocked or require explicit confirmation in production.

### 3. Supply Chain Attacks
*   **Threat**: Malicious dependency introducing a backdoor.
*   **Mitigation**:
    *   Minimal dependency tree.
    *   Open Source (users can audit requirements).
    *   (Future) SBOM and signed binaries.

### 4. Data Leaks via Logs
*   **Threat**: Application logs containing connection strings or query results.
*   **Mitigation**:
    *   Structured logging (`tracing`) with redaction.
    *   Query results are NOT logged by default.
    *   Logs are stored locally in user's home directory (`~/.qoredb/logs`).
