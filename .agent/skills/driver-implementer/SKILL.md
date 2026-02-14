---
name: driver-implementer
description: Workflow for adding a new database driver (e.g., Redis, SQLite, Cassandra) to QoreDB engine. Use when the user asks to "add support for X database" or "create a driver".
---

# Driver Implementer

This skill guides the implementation of a new `DataEngine` driver in QoreDB.

## Workflow

### 1. Driver Scaffold (Rust)

1.  **Create Driver File**:
    Create a new file in `src-tauri/src/engine/drivers/<driver_name>.rs` using the `assets/driver_template.rs`.
    - Rename struct `NewDriver` to `<DriverName>Driver`.
    - Update `driver_id()` to return the snake_case ID (e.g., "sqlite").
    - Update `driver_name()` to return the Display Name (e.g., "SQLite").

2.  **Declare Module**:
    In `src-tauri/src/engine/drivers/mod.rs`:
    - Add `pub mod <driver_name>;`

3.  **Register Driver**:
    In `src-tauri/src/lib.rs` (AppState::new):
    - Import the new module: `use engine::drivers::<driver_name>::<DriverName>Driver;`
    - Register it: `registry.register(Arc::new(<DriverName>Driver::new()));`

### 2. Dependency Management

1.  **Add Crate**:
    - Ask the user to add the Rust crate: `cargo add <crate_name> --package qoredb` (usually in `src-tauri`).
    - _Note_: Ensure `tokio` support is enabled for the crate if available, as QoreDB is async.

### 3. Implementation Guide

Implement the `DataEngine` trait methods in this order:

1.  **Connection**: `test_connection` and `connect`. (You'll need a way to map `SessionId` -> `Connection` using a `RwLock<HashMap>`).
2.  **Metadata**: `list_namespaces` and `list_collections`.
3.  **Query Execution**: `execute` (Parsing results into `QueryResult`).
4.  **Schema**: `describe_table` (Mapping types to QoreDB types).

## Template

### Driver Implementation (`src-tauri/src/engine/drivers/<name>.rs`)

Use the asset `assets/driver_template.rs` as a base.

```rust
// Key imports
use crate::engine::traits::DataEngine;
use crate::engine::types::{...};
```

## Checklist

- [ ] `Cargo.toml`: Added driver dependency (e.g. `rusqlite`, `redis`)
- [ ] `drivers/<name>.rs`: Created struct implementing `DataEngine`
- [ ] `drivers/mod.rs`: Module declared
- [ ] `lib.rs`: Driver registered in `DriverRegistry`
- [ ] `test_connection`: Implemented and verified
