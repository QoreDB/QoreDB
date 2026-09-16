# Architecture for contributors

This is a map of the current implementation, not a proposal. Follow the linked
entry points and search callers before changing a contract.

## Package boundaries

| Surface | Source of truth | Responsibility |
| --- | --- | --- |
| React application | [src](../../src/) | Views, local UI state, hooks, translations |
| Frontend transport | [tauri bindings](../../src/lib/tauri/), [transport](../../src/lib/transport.ts) | Typed commands, desktop/web routing, stream decoding |
| Desktop shell | [commands](../../src-tauri/src/commands/), [lib.rs](../../src-tauri/src/lib.rs) | Tauri state, IPC registration, desktop events and plugins |
| Engine contract | [qore-core](../../src-tauri/crates/qore-core/src/) | `DataEngine`, registry, shared types, errors, cursors |
| Database access | [qore-drivers](../../src-tauri/crates/qore-drivers/src/) | Drivers, sessions, cancellation, protocol helpers, SSH |
| SQL utilities | [qore-sql](../../src-tauri/crates/qore-sql/src/) | Safety classification, generation, connection URLs |
| Query representation | [qore-query](../../src-tauri/crates/qore-query/src/) | AST, compilers, dialects |
| Shared service | [qore-service](../../src-tauri/crates/qore-service/src/) | Connections, query preflight/execution, mutations, vault, governance, cache |
| Headless entry points | [CLI](../../src-tauri/crates/qore-cli/README.md), [MCP](../../src-tauri/crates/qore-mcp/README.md), [server](../../src-tauri/crates/qore-server/README.md) | Transport and authorization for their callers |

`src-tauri/src/engine/mod.rs` is a compatibility facade that re-exports the
workspace crates. Put a driver fix in `qore-drivers`, not in that facade.
`src/lib/tauri.ts` similarly re-exports frontend binding modules; it is not the
location of every binding implementation.

## Trace a query

1. A component/hook calls a typed binding such as `executeQuery` in
   [tauri/query.ts](../../src/lib/tauri/query.ts).
2. The binding routes through the frontend transport. The desktop path invokes
   `execute_query` in [commands/query.rs](../../src-tauri/src/commands/query.rs);
   the web path uses the transport's server implementation.
3. The desktop adapter assembles state and uses shared query preflight and
   completion logic in [service/query.rs](../../src-tauri/crates/qore-service/src/query.rs).
   Desktop-specific orchestration, plugins, and events remain in the adapter.
4. [ServiceContext](../../src-tauri/crates/qore-service/src/context.rs) holds the
   registry, session/query managers, policy, cache, interceptor, and vault.
   A driver implements [DataEngine](../../src-tauri/crates/qore-core/src/traits.rs).
5. Results return through typed responses or streams. The desktop stream uses
   MessagePack envelopes; change the Rust producer and TypeScript decoder together.

Shared behavior also has headless callers. Adding a UI check alone does not
protect CLI/MCP/server access. Preserve the query preflight, read-only policy,
production acknowledgement, rate limits, masking, and audit/redaction behavior
in every affected path. See [production safety](../security/PRODUCTION_SAFETY.md)
and the [threat model](../security/THREAT_MODEL.md).

## Common change boundaries

| Change | Other places to inspect |
| --- | --- |
| Rust response or enum | TypeScript binding types, serialized names/defaults, all transport adapters |
| Driver capability | Trait defaults, registry output, frontend metadata/capabilities, relevant UI |
| Mutation classification | SQL/protocol safety, shared service preflight, desktop and headless callers |
| Cache or pagination | Query key, namespace isolation, invalidation, cursor metadata, result rendering |
| New Premium behavior | File SPDX, backend license enforcement, Core and Pro feature combinations |
| New UI text | `src/i18n.ts`, all `src/locales/*.json`, existing component conventions |

The tree mixes Core and Premium within some crates. Cargo package metadata and
feature names do not determine every source file's license. See
[licensing](LICENSING.md) before moving code across modules.
