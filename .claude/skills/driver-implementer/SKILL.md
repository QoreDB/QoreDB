---
name: driver-implementer
description: Workflow for adding a new database driver to the QoreDB engine. Use when the user asks to "add support for X database" or "create a driver".
---

# Driver Implementer

Guides the implementation of a new `DataEngine` driver. The backend is a Cargo
workspace: drivers live in the `qore-drivers` crate, the trait and shared types
in `qore-core`, and driver registration in `qore-service`.

Already implemented (don't re-add): Postgres, MySQL, MariaDB, MongoDB, Redis,
SQLite, DuckDB, CockroachDB, SQL Server, Supabase, ClickHouse. Copy the closest
existing driver as a starting point (`sqlite.rs` is the smallest SQL one).

## Workflow

### 1. Driver scaffold (Rust)

1. **Create the driver file**:
   `src-tauri/crates/qore-drivers/src/drivers/<driver_name>.rs`, using
   `assets/driver_template.rs` as a skeleton (start from an existing driver for
   the full trait surface).
   - Start with the SPDX header (`// SPDX-License-Identifier: Apache-2.0`).
   - Rename the struct to `<DriverName>Driver`.
   - `driver_id()` returns the snake_case id (e.g. `"sqlite"`).
   - `driver_name()` returns the display name (e.g. `"SQLite"`).

2. **Declare the module**:
   In `src-tauri/crates/qore-drivers/src/drivers/mod.rs`, add
   `pub mod <driver_name>;` and re-export the driver struct.

3. **Register the driver**:
   In `src-tauri/crates/qore-service/src/context.rs`, where the other drivers are
   registered:
   - Import it in the `use qore_drivers::drivers::{ ... }` block.
   - Add `registry.register(Arc::new(<DriverName>Driver::new()));`.

### 2. Dependency management

- Add the crate to the driver crate: `cargo add <crate_name> --package qore-drivers`.
- Enable `tokio`/async features when the crate offers them — QoreDB is async.

### 3. Implementation guide

The `DataEngine` trait lives in `src-tauri/crates/qore-core/src/traits.rs`, with
its types in `qore-core::types`. Many methods have default implementations
(routines, triggers, sequences, events, maintenance return empty/unsupported by
default) — implement only what the backend supports. Start with:

1. **Identity**: `driver_id`, `driver_name`.
2. **Connection**: `test_connection`, `connect`, `disconnect`, `ping`
   (map `SessionId -> Connection` via a `RwLock<HashMap<...>>`).
3. **Metadata**: `list_namespaces`, `list_collections`.
4. **Query**: `execute` (parse results into `QueryResult`) and its variants.
5. **Schema**: `describe_table` (map backend types to QoreDB types).

## Checklist

- [ ] `qore-drivers/Cargo.toml`: dependency added
- [ ] `qore-drivers/src/drivers/<name>.rs`: struct implementing `DataEngine` (+ SPDX)
- [ ] `qore-drivers/src/drivers/mod.rs`: module declared + re-exported
- [ ] `qore-service/src/context.rs`: driver registered in the registry
- [ ] `test_connection`: implemented and verified
