# QoreDB plugin development

Tooling for writing **executable plugins** — sandboxed WASM modules that extend
QoreDB's behaviour through lifecycle hooks. These crates are standalone: they
are **not** part of the `src-tauri` workspace and never built by `tauri dev`.

## Layout

- `sdk/` — the `qoredb-plugin-sdk` crate. Plugins depend on it; it hides the
  host ABI (linear-memory marshalling, packed return values, capability call
  helpers) behind a typed Rust API.
- `cli/` — the `qoredb-plugin` binary: scaffold, build and install plugins
  from the terminal. See [CLI usage](#cli-usage) below.
- `examples/sql-linter/` — a worked example: blocks `UPDATE`/`DELETE` without
  a `WHERE` clause.
- [`ABI.md`](./ABI.md) — host/guest ABI reference (required exports, packed
  return shape, error codes).

## Quick start

```sh
# One-off
rustup target add wasm32-unknown-unknown
cargo install --path plugins-dev/cli   # builds the qoredb-plugin tool

# New plugin
qoredb-plugin new acme.hello
cd acme.hello
# edit src/lib.rs to taste, then
qoredb-plugin build
qoredb-plugin install     # copies into QoreDB's plugins folder
```

Open QoreDB → Settings → Plugins; the new plugin appears. Granting consent for
any capability it requested (see below) brings its hooks online on the next
query.

## Manifest

A plugin is a folder containing `plugin.json` + the built `.wasm` module. The
manifest spells out what the plugin contributes (snippets, themes, templates,
result viewers, commands) and what capabilities its WASM module asks for.

```json
{
  "$schema": "../../plugin.schema.json",
  "id": "acme.audit",
  "name": "Audit Trail",
  "version": "1.0.0",
  "author": "Acme Corp",
  "description": "POSTs every executed mutation to an audit endpoint.",
  "qoredb": ">=0.1.29",
  "runtime": {
    "abiVersion": 1,
    "entry": "plugin.wasm",
    "integrity": "sha256-e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "hooks": ["postExecute"],
    "capabilities": {
      "log": true,
      "notify": true,
      "http": { "allowedHosts": ["audit.acme.test"] },
      "secrets": ["audit-token"]
    }
  },
  "contributes": {
    "commands": [{ "id": "ping", "label": "Ping audit endpoint" }]
  }
}
```

See [`plugin.schema.json`](../plugin.schema.json) for the full schema (VS Code
picks it up via `$schema`).

### `runtime` block

| Field | Required | Description |
| --- | --- | --- |
| `abiVersion` | yes | Host ABI version the plugin was built against. Current: **1**. |
| `entry` | yes | `.wasm` filename, bare (no `/` or `..`), sitting next to `plugin.json`. |
| `integrity` | no | `sha256-<64 hex>` of the WASM bytes. When present, the host **refuses to load** a tampered module. Surfaced in the UI as `Signed` vs `Unsigned`. |
| `hooks` | no | Lifecycle hooks the module subscribes to: `preExecute`, `postExecute`. |
| `capabilities` | no | Host surfaces the module wants. Default: none — a hook with no capabilities is pure compute. |

### Capabilities

Every capability is **off by default**. The user grants them at install time
via the consent dialog; a granted capability is revocable from the plugin's
detail view. Capabilities a host fn checks at call time, so a revoked grant
becomes an `ERR_DENIED` immediately — no reload needed.

| Capability | Manifest shape | Granted plugin can… |
| --- | --- | --- |
| `log` | `"log": true` | Write to QoreDB's tracing log (`qoredb_log`). |
| `notify` | `"notify": true` | Surface a toast (`qoredb_notify`). |
| `storage` | `"storage": true` | Read/write a per-plugin KV store, capped at 1 MiB / 1024 entries / 64 KiB per value (`qoredb_kv_get` / `_set` / `_del`). |
| `queryRead` | `"queryRead": true` | Pull the JSON-serialised rows of the current query result inside `postExecute` (`qoredb_query_read`). Capped at 1 MiB; past that the call returns `0`. |
| `http` | `"http": { "allowedHosts": ["api.example.com"], "allowPrivateNetworks": false }` | Issue outbound HTTPS to the named hosts (`qoredb_http_request`). Refusing private/loopback/cloud-metadata addresses by default is enforced post-DNS — set `allowPrivateNetworks: true` only if you really need to (audited in the consent UI). |
| `fs` | `"fs": { "scope": "pluginData" }` | Read/write files under `<plugin>/data/` only (`qoredb_fs_read` / `_write` / `_delete`). 4 MiB per file. |
| `secrets` | `"secrets": ["api-token"]` | Pull named secrets out of the OS keyring (`qoredb_secret_get`). The host filters to names declared here, so a tampered consent file can't widen the set. |

### Contributions

`contributes` carries the data the plugin exposes to the rest of QoreDB:

- **`snippets`** — labelled SQL templates that appear in the snippets palette.
- **`connectionTemplates`** — pre-filled connection presets, filtered by
  `driver`; selected from the connection modal.
- **`themes`** — colour themes mapping `--q-*` CSS variables, in light/dark
  pairs. Non-design-token variables are rejected at install time.
- **`resultViewers`** — declarative cell renderers picked by column type or
  name pattern. Renderers: `json-tree`, `image`, `chart`, `map`.
- **`commands`** — user-invocable actions that fire the `command` hook (see
  [SDK usage](#writing-a-hook)). Requires a `runtime` block.

## Writing a hook

```rust
use qoredb_plugin_sdk::{
    export_command, export_post_execute, export_pre_execute,
    log, notify, storage, http,
    CommandEnvelope, Decision, HookContext, LogLevel, NotifyLevel,
    PostExecuteEnvelope,
};

fn pre(ctx: HookContext) -> Decision {
    if ctx.is_mutation && !ctx.query.to_uppercase().contains("WHERE") {
        log(LogLevel::Warn, "blocking unscoped mutation");
        return Decision::block("mutation without WHERE");
    }
    Decision::allow()
}

fn post(envelope: PostExecuteEnvelope) {
    if !envelope.result.success {
        notify(NotifyLevel::Error, "Query failed; please retry.");
        return;
    }
    if envelope.context.is_mutation {
        // Persist a counter; storage capability must be granted.
        let prev: u32 = storage::get("mutations")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        storage::set("mutations", &(prev + 1).to_string());
    }
}

fn command(env: CommandEnvelope) -> serde_json::Value {
    // env.id is the bare command id; the host has already resolved
    // the namespaced "<plugin-id>::<command-id>" to this.
    match env.id.as_str() {
        "ping" => {
            // http capability + allowlist + (by default) public DNS.
            let resp = http::get("https://audit.acme.test/ping");
            serde_json::json!({ "ok": resp.is_some() })
        }
        _ => serde_json::Value::Null,
    }
}

export_pre_execute!(pre);
export_post_execute!(post);
export_command!(command);
```

A `pre_execute` hook returns `Decision::allow()`, `Decision::warn(msg)` or
`Decision::block(reason)`. A `Block` stops the query before it runs. A `Warn`
lets it run and surfaces a toast to the user.

`post_execute` observes; its return value is ignored. Failures and timeouts
are dispatched too — check `envelope.result.success` and `.error`.

`command` is fired by a user clicking a contributed command in the UI. Its
JSON return value is shown back to the user.

### Capability helpers

Every capability comes with a helper in the SDK that no-ops if the capability
hasn't been granted — write defensively, the host will refuse silently anyway:

| Helper | Capability |
| --- | --- |
| `log(level, msg)` | `log` |
| `notify(level, msg)` | `notify` |
| `storage::{get, set, delete}` | `storage` |
| `query_read()` (in `post_execute`) | `queryRead` |
| `http::{get, post, request}` | `http` |
| `fs::{read, write, delete}` | `fs` |
| `secrets::get(name)` | `secrets` |

## Debugging

- **Tracing log** — every plugin denial and every capability-grant violation
  is logged at `warn` against the `plugins` target. Run QoreDB with
  `RUST_LOG=plugins=warn` to surface them.
- **Storage file** — `~/Library/Application Support/com.qoredb.app/plugins/<id>/storage.json`
  (macOS) or the platform equivalent.
- **Circuit breaker** — three consecutive hook traps disable the plugin for
  the session; a toast announces it. Restart QoreDB or `reload` to re-arm.
- **Wall-clock timeouts** — `pre_execute` is capped at 500 ms, `post_execute`
  at 5 s. Past that, the hook is recorded as a failure (counts towards the
  circuit breaker).
- **Resource budgets** — every invocation gets fresh fuel (≈ 50 M
  instructions) and a fixed memory cap (16 MiB). Exhaustion shows up as
  `PluginError::BudgetExceeded` in the log.

## CLI usage

```sh
qoredb-plugin new acme.hello   # scaffolds a plugin folder + Cargo crate
qoredb-plugin build            # cargo build + sha256 + updates integrity
qoredb-plugin install          # copies the folder into the QoreDB plugins dir
```

`build` writes the freshly computed sha256 into `runtime.integrity` so the
host refuses to load a tampered binary. `install` copies the folder under
QoreDB's data directory; refresh the Plugins panel in-app to pick it up.

## Distribution

QoreDB has a public marketplace at <https://qoredb.com/marketplace>, backed
by the [`qoredb-plugins-registry`](https://github.com/qoredb/qoredb-plugins-registry)
GitHub repo. Submit a plugin through the
[submission form](https://qoredb.com/marketplace/submit) or by opening a PR
against the registry; a maintainer reviews the manifest, the requested
capabilities and the WASM integrity before it lands in the catalog.

You can still distribute a plugin as a folder (tar, zip, git clone) — the
user installs it via the GUI's **Install plugin** action or
`qoredb-plugin install`. The `integrity` field remains the only end-to-end
binding between the manifest's identity and the bytes the host will run, so
sign your plugin either way.

## What's next

See [`doc/todo/PLUGINS_HARDENING.md`](../doc/todo/PLUGINS_HARDENING.md) at the
repo root for the roadmap (perf migration to wasmtime, registry, …).
