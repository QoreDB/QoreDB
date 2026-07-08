---
name: qa-security-auditor
description: Security and QA auditing workflow for QoreDB. Use when the user asks to "check security", "audit code", or before merging critical features. Checks for sensitive data leaks, dangerous permissions, and unsafe SQL patterns.
---

# QA & Security Auditor

Security and quality audit of the codebase, focused on the specific risks of a
desktop database client. See `doc/security/THREAT_MODEL.md` and
`doc/security/PRODUCTION_SAFETY.md` for the broader model.

## Audit checklist

### 1. Sensitive data exposure (logs)

**Risk**: credentials appearing in logs.

- [ ] Search for `console.log`, `println!`, `dbg!`, `tracing`/`log` macros
      containing "password", "secret", "key", "token".
- [ ] Verify connection strings are redacted before logging.

### 2. Dangerous operations

**Risk**: accidental data loss.

- [ ] Verify that commands performing destructive actions (drop/truncate/delete)
      go through the safety layer: `src-tauri/crates/qore-service/src/governance.rs`
      and `src-tauri/src/commands/policy.rs` (Mongo specifics in
      `src-tauri/crates/qore-drivers/src/mongo_safety.rs`).
- [ ] Ensure the frontend requires confirmation before invoking them
      (`useConfirmState` hook).

### 3. Tauri permissions (`src-tauri/capabilities/`)

**Risk**: excessive system access.

- [ ] Check `src-tauri/capabilities/default.json`.
- [ ] Ensure the `fs` scope is limited (no `$HOME/*` unless strictly necessary).
- [ ] Ensure the `shell` scope doesn't allow arbitrary command execution.

### 4. SQL / NoSQL injection

**Risk**: malicious queries.

- [ ] Rust: query arguments are bound parameters, not string concatenation
      (`format!("SELECT * FROM {}", table)` is dangerous).
- [ ] Rust: dynamic identifiers (table/column names) are validated/escaped
      through the safety layer, not interpolated raw.

## Workflow

1. **Pattern search**: grep for `console.log` / `println!` / `dbg!` in the
   changed files.
2. **Verify safety checks**: read the relevant command; does it route destructive
   actions through `governance.rs` / `policy.rs` before executing?
3. **Check permissions**: read `src-tauri/capabilities/default.json` if new
   filesystem or shell features were added.

## Common fixes

**Redacting logs (Rust):**

```rust
// BAD
println!("Connecting to {}", password);

// GOOD
tracing::info!("Connecting to database with user {}", user); // no secrets
```

**Bound parameters over interpolation (Rust):**

```rust
// BAD — SQL injection via interpolation
engine.execute(&format!("SELECT * FROM {table} WHERE id = {id}")).await;

// GOOD — bound parameter; identifier validated through the safety layer
engine.execute_with_params("SELECT * FROM users WHERE id = $1", &[id]).await;
```
