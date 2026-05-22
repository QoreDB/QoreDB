# QoreDB plugin development

Tooling for writing **executable plugins** — sandboxed WASM modules that extend
QoreDB's behaviour through lifecycle hooks. These crates are standalone: they
are **not** part of the `src-tauri` workspace and never built by `tauri dev`.

## Layout

- `sdk/` — the `qoredb-plugin-sdk` crate. Plugins depend on it; it hides the
  host ABI (linear-memory marshalling) behind a typed API.
- `examples/sql-linter/` — a worked example: blocks `UPDATE`/`DELETE` without a
  `WHERE` clause.

## Building a plugin

1. Install the WASM target once:

   ```sh
   rustup target add wasm32-unknown-unknown
   ```

2. Build, from the plugin's directory:

   ```sh
   cargo build --release --target wasm32-unknown-unknown
   ```

3. The module is at
   `target/wasm32-unknown-unknown/release/<crate_name>.wasm`
   (e.g. `qoredb_sql_linter.wasm`).

## Installing a plugin

A plugin on disk is a folder containing:

- `plugin.json` — the manifest (its `runtime.entry` names the `.wasm` file);
- the built `.wasm` module, next to it.

Place the built module beside `plugin.json` so its filename matches
`runtime.entry`, then install the folder via **QoreDB → Settings → Plugins →
Install**.

## Writing a hook

```rust
use qoredb_plugin_sdk::{export_pre_execute, Decision, HookContext};

fn check(ctx: HookContext) -> Decision {
    if ctx.is_mutation && !ctx.query.to_uppercase().contains("WHERE") {
        return Decision::block("mutation without WHERE");
    }
    Decision::allow()
}

export_pre_execute!(check);
```

A `pre_execute` hook returns `Decision::allow()`, `Decision::warn(msg)` or
`Decision::block(reason)`. A `Block` stops the query before it runs.

Hooks are pure compute in this release: no capabilities (network, filesystem,
query access) are granted yet — those arrive in a later phase.
