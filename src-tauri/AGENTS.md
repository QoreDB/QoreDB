# Rust workspace

- Driver contracts/types/errors: `crates/qore-core/`. Implementations, sessions,
  protocol helpers: `crates/qore-drivers/`. SQL safety and generation:
  `crates/qore-sql/`. Query AST/compiler: `crates/qore-query/`.
- Shared application behavior lives in `crates/qore-service/`; keep it free of
  Tauri dependencies. `ServiceContext` registers the compiled-in drivers.
- `src/commands/` adapts desktop IPC; `src/lib.rs` registers handlers.
  `src/engine/mod.rs` re-exports workspace code for compatibility.
- CLI, MCP, and server are separate workspace packages. Changes to service
  behavior must account for their callers, not just desktop commands.
- Follow [the driver guide](../doc/development/DRIVERS.md) for feature forwarding,
  capabilities, protocol classification, and frontend wiring.
- Propagate typed errors with `?`; engine errors live in `qore-core/src/error.rs`
  and service errors in `qore-service/src/error.rs`. Preserve secret redaction.
- Features are per package. `qoredb` defaults to Core, but driver/service defaults
  include broad driver sets. Use explicit `-p`, `--no-default-features`, and the
  relevant features for focused checks; see [testing](../doc/development/TESTING.md).
- Database integration tests can silently skip unavailable services. Set the
  corresponding `QOREDB_TEST_*_REQUIRED=true` for claimed live coverage.
- Premium desktop changes need the relevant `pro` build/test path as well as
  Core compatibility. Never infer file licensing solely from Cargo features.
- Do not edit `vendor/` unless the task concerns the maintained dependency patch.
