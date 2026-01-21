# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.0.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

If you discover a security vulnerability within QoreDB, please send an e-mail to `qoredb@gmail.com`. All security vulnerabilities will be promptly addressed.

## Principles

QoreDB is designed with a **Local-First, Privacy-First** architecture:

1.  **Secrets Management**: Database credentials are stored in the OS Keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service). They are never synced to the cloud or logged.
2.  **No Telemetry**: By default, QoreDB collects no telemetry. Any future telemetry will be strictly opt-in.
3.  **Read-Only by Default**: Connections can be marked as "Read-Only" to prevent accidental data modification.
4.  **Production Safety**: "Production" environments require explicit confirmation for dangerous SQL operations (DROP, ALTER, TRUNCATE, UPDATE/DELETE without WHERE).
