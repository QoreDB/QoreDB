# Add or extend a database driver

A driver is complete when its identity, capabilities, safety classification,
connection UI, and supported entry points agree, and the claimed behavior has
appropriate test evidence. Start with the [architecture](ARCHITECTURE.md) and
[current limitations](../tests/DRIVER_LIMITATIONS.md).

## Choose the implementation boundary

Use an existing protocol family when the database is wire-compatible, but check
its differences explicitly. Examples in
[qore-drivers/src/drivers](../../src-tauri/crates/qore-drivers/src/drivers/):

- PostgreSQL-compatible services use `pg_compat/` and wrappers such as `neon.rs`.
- MySQL and Redis variants can use constructors on their existing driver.
- Search engines share `search_compat/`; Snowflake/BigQuery share `warehouse_compat/`.
- A new protocol needs its own implementation and safety handling.

Compatibility is not full server equivalence. Define namespace semantics,
authentication/TLS, transactions, streaming, cancellation, mutation support,
pagination, and known limitations before copying a neighboring driver's flags.
Pick one stable lowercase driver ID and use it across Rust, TypeScript, persisted
connections, and tests. Check [licensing](LICENSING.md) for new files.

## Backend wiring

1. Implement [DataEngine](../../src-tauri/crates/qore-core/src/traits.rs) under
   `qore-drivers/src/drivers/`. Read the trait instead of copying a method list
   from a guide: required methods and optional defaults evolve. Use shared
   [types](../../src-tauri/crates/qore-core/src/types.rs) and `EngineError`.
2. Implement connection lifecycle, health, namespaces, collections, schema, query
   execution, table browsing, and cancellation behavior. Keep bound values
   separate from quoted identifiers. Preserve integer/decimal precision and
   distinguish nulls, empty values, and errors in decoding.
3. Declare the capabilities actually implemented. `capabilities()` aggregates
   `supports_*`, `cancel_support()`, and `pagination_capability()`. Unsupported
   operations must retain an explicit unsupported result, not a false success.
   Describe best-effort cancellation and pagination ordering honestly.
4. Add an optional `driver-<id>` feature to
   [qore-drivers/Cargo.toml](../../src-tauri/crates/qore-drivers/Cargo.toml), include
   it in `all-drivers` if shipped, and gate the module/imports in
   [drivers/mod.rs](../../src-tauri/crates/qore-drivers/src/drivers/mod.rs).
5. Forward the feature through
   [qore-service/Cargo.toml](../../src-tauri/crates/qore-service/Cargo.toml), then
   register the implementation under the same feature in
   [context.rs](../../src-tauri/crates/qore-service/src/context.rs).
6. Forward the feature in the manifests for
   [qore-cli](../../src-tauri/crates/qore-cli/Cargo.toml),
   [qore-mcp](../../src-tauri/crates/qore-mcp/Cargo.toml), and
   [qore-server](../../src-tauri/crates/qore-server/Cargo.toml).
   The desktop currently enables `all-drivers` through its dependencies. Inspect
   these manifests rather than assuming every package has identical defaults.

## Safety and connection handling

- For SQL dialects, inspect driver-ID dispatch in
  [safety.rs](../../src-tauri/crates/qore-sql/src/safety.rs) and
  [generator.rs](../../src-tauri/crates/qore-sql/src/generator.rs).
  Test classification of reads, writes, dangerous statements, and parse failures.
- For non-SQL protocols, inspect
  [service/query.rs](../../src-tauri/crates/qore-service/src/query.rs) and existing
  protocol classifiers in `qore-drivers`. A new ID must not accidentally fall
  through to an unrelated SQL classifier or bypass read-only enforcement.
- If URLs are supported, update
  [connection_url.rs](../../src-tauri/crates/qore-sql/src/connection_url.rs) and the
  frontend URL/detection helpers together. Do not invent a scheme when a vendor
  uses an existing one; ambiguous flavors may require explicit selection.
- Reuse connection options and vault handling. Check TLS verification, SSH/proxy
  support, credential redaction, and error paths. Cloud credentials and real
  query results do not belong in fixtures or logs.

## Frontend wiring

| File | What to update |
| --- | --- |
| [drivers.ts](../../src/lib/connection/drivers.ts) | `Driver`, `DRIVERS`, data model, naming, icon, ports, identifier rules and supported query builders |
| [driverCapabilities.ts](../../src/lib/connection/driverCapabilities.ts) | Static schema-object capabilities and protocol-family/dialect helpers |
| [connectionUrls.ts](../../src/lib/connection/connectionUrls.ts) | URL support and placeholders, when applicable |
| [dsnDetector.ts](../../src/lib/connection/dsnDetector.ts) | Scheme/host detection and ambiguity handling |
| [Connection UI](../../src/components/Connection/) | Required fields, validation, defaults and driver-specific options |
| [database assets](../../public/databases/) | Icon referenced by the descriptor |
| [locales](../../src/locales/) | New visible strings in every supported language |

Search for exhaustive maps/switches and persisted-ID assumptions after adding the
enum value. TypeScript will catch many `Record<Driver, ...>` omissions, but not
string comparisons. UI capabilities must agree with the backend's runtime flags.

## Verification and delivery

Start with a focused compile/test of the new feature, then its service and entry
point wiring. See [testing](TESTING.md) for concrete commands; replace
`driver-sqlite` with the feature under development. Include at least:

- Deterministic unit/mock tests for parsing, serialization, errors, capability
  flags, safety classification, and URL detection where applicable.
- A live lifecycle/query/schema test in
  [integration_databases.rs](../../src-tauri/tests/integration_databases.rs) when a
  service is available. Extend its required-service mechanism so missing services
  cannot masquerade as verified support. Add the disposable local service to
  [docker-compose.yml](../../docker-compose.yml) if appropriate.
- Tests for advertised mutations/transactions/streaming/pagination and failure
  cases. A protocol stand-in validates the shared protocol, not vendor behavior.
- A connection-form and browse/query smoke test in the desktop app; headless
  coverage for the entry points the driver is intended to support.

Record unsupported operations and whether tests used mocks, a compatible
stand-in, or the real server in [DRIVER_LIMITATIONS.md](../tests/DRIVER_LIMITATIONS.md).
Update [README](../../README.md) and [FEATURES.csv](../FEATURES.csv) when shipping
support. A roadmap entry alone is not evidence of implementation.
