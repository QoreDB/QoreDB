---
name: qa-security-auditor
description: Security and QA auditing workflow for QoreDB. Use when the user asks to "check security", "audit code", or before merging critical features. Checks for sensitive data leaks, dangerous permissions, and unsafe SQL patterns.
---

# QA & Security Auditor

This skill performs a security and quality audit of the codebase, focusing on the specific risks of a desktop database client.

## Audit Checklist

### 1. Sensitive Data Exposure (Logs)
**Risk**: Credentials appearing in logs.
*   [ ] Search for `console.log`, `println!`, `dbg!` containing "password", "secret", "key", "token".
*   [ ] Verify that connection strings are redacted before logging.

### 2. Dangerous Operations
**Risk**: Accidental data loss.
*   [ ] Verify that any Rust command calling `drop_database`, `delete_row`, or `truncate` checks the `SafetyPolicy`.
*   [ ] Ensure the frontend invokes a confirmation modal (e.g., `useDoubleCheck`) before calling these commands.

### 3. Tauri Permissions (`src-tauri/capabilities/`)
**Risk**: Excessive system access.
*   [ ] Check `default.json` or `base.json`.
*   [ ] Ensure `fs` scope is limited (no `$HOME/*` unless absolutely necessary).
*   [ ] Ensure `shell` scope doesn't allow arbitrary command execution.

### 4. SQL Injection / NoSQL Injection
**Risk**: Malicious queries.
*   [ ] Rust: Ensure `query` arguments are bound parameters, not string concatenation (e.g. `format!("SELECT * FROM {}", table)` is dangerous).
*   [ ] Rust: For dynamic table names, ensure validation/escaping via `sql_safety.rs`.

## Workflow

1.  **Run Pattern Search**: Use `grep_search` to find `console.log` or `println!` in modified files.
2.  **Verify Policy Checks**: Read the relevant Rust command file. Does it import `SafetyPolicy`? Does it call `policy.check(...)`?
3.  **Check Permissions**: Read `src-tauri/capabilities/default.json` if new filesystem features were added.

## Common Fixes

**Redacting Logs (Rust):**
```rust
// BAD
println!("Connecting to {}", password);

// GOOD
info!("Connecting to database with user {}", user); // standard log without secrets
```

**Checking Safety (Rust):**
```rust
// BAD
pub async fn drop_table(...) {
    engine.execute("DROP TABLE ...").await;
}

// GOOD
pub async fn drop_table(state: State<'_, AppState>, ...) {
    state.policy.enforce_dangerous_action()?; // Returns error if not confirmed/allowed
    engine.execute("DROP TABLE ...").await;
}
```
