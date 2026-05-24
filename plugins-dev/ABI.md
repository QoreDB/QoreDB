# QoreDB plugin ABI v1

Reference for the host/guest boundary used by QoreDB executable plugins. The
SDK hides every detail in this document behind typed Rust calls; you only
need this page if you're writing a plugin in a language other than Rust, or
debugging an ABI mismatch.

The current ABI version is **`1`**. A plugin's manifest must declare
`runtime.abiVersion: 1` — anything else is refused at load time.

## Module shape

A QoreDB plugin is a WebAssembly module (MVP feature set, 32-bit) that:

- **Exports** at least `memory` and `qoredb_alloc`; optionally
  `pre_execute`, `post_execute` and `command`.
- **Imports** from the `env` namespace any host functions corresponding to
  capabilities it intends to call. Importing a function the manifest didn't
  request still links — capability enforcement happens at call time, not
  link time.

The module must instantiate cleanly: anything thrown from a `start` function
fails the load.

## Required exports

### `memory`

The plugin's linear memory. The host reads inputs from it and writes
return-value payloads into it through `qoredb_alloc`. No upper bound on the
exported size, but the runtime caps growth to **256 pages** (16 MiB).

### `qoredb_alloc`

```
qoredb_alloc(len: i32) -> i32
```

Reserve `len` bytes inside the guest's linear memory and return the offset
(an i32 pointer). The host calls this twice per invocation:

1. Once before a hook, to write the JSON input where the hook will read it.
2. Possibly from inside a host function (e.g. `qoredb_storage_get`) to
   place the returned bytes into guest memory before handing back a packed
   pointer.

The host **never frees** what `qoredb_alloc` reserved. Each hook runs in a
**fresh store** (new linear memory, new fuel budget), so the buffers are
reclaimed wholesale at the end of the call. A naïve bump allocator that
forgets the buffer is correct and recommended:

```rust
pub fn alloc(len: i32) -> i32 {
    let mut buf: Vec<u8> = Vec::with_capacity(len.max(0) as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as i32
}
```

Returning `0` for an alloc failure is **not** acceptable: the host treats `0`
as a valid offset and will scribble there. Trap (`unreachable`) instead — the
host catches it as `PluginError::Trap`.

## Optional hook exports

### `pre_execute`

```
pre_execute(ptr: i32, len: i32) -> i64
```

The host writes a JSON `HookContext` at `[ptr, ptr+len)` and calls
`pre_execute`. The plugin returns a **packed pointer** (see below) to a JSON
`Decision`. A module that does not export `pre_execute` is treated as
returning `Decision::Allow` for every query.

`HookContext`:

```json
{
  "query": "SELECT 1",
  "driverId": "postgres",
  "environment": "Development",
  "operationType": "Select",
  "isMutation": false,
  "isDangerous": false,
  "readOnly": true
}
```

`Decision` (one of):

```json
{"kind": "allow"}
{"kind": "warn",  "message": "..."}
{"kind": "block", "reason":  "..."}
```

### `post_execute`

```
post_execute(ptr: i32, len: i32)
```

Fires after a query — successful or not. Takes a JSON envelope at
`[ptr, ptr+len)`, returns nothing. The host swallows traps and `Err` returns
(both count toward the circuit-breaker).

Envelope:

```json
{
  "context": { ...HookContext... },
  "result": {
    "success": true,
    "executionTimeMs": 12,
    "rowCount": 42,
    "error": null
  }
}
```

Row contents are **not** in the envelope: pull them through `qoredb_query_read`
when `queryRead` is granted.

### `command`

```
command(ptr: i32, len: i32) -> i64
```

Fires when the user clicks a contributed `command` in the UI. The plugin
receives a JSON envelope and returns a packed pointer to the JSON value it
wants surfaced back to the user. Returning `0` is shorthand for `null`.

Envelope:

```json
{ "id": "lint-current", "args": {...} }
```

`id` is the bare command id — the namespaced `<plugin>::<id>` form is
resolved by the host before dispatch.

## Packed return shape

Hook returns and most host fn returns use a **packed `i64`** encoding a
`(ptr, len)` pair:

```
packed = (ptr << 32) | (len & 0xFFFF_FFFF)
```

- The high 32 bits hold the offset into the guest memory.
- The low 32 bits hold the byte length.

A return of `0` is the "no payload" sentinel — used by storage-style getters
to indicate a missing key, or by `command` to mean "the plugin chose to
surface nothing."

### Reading what the host wrote back

After a host fn that returns a packed `i64`, the plugin unpacks it:

```rust
let packed = qoredb_storage_get(key_ptr, key_len);
if packed == 0 {
    return None; // key missing
}
let ptr = (packed >> 32) as u32 as i32;
let len = (packed & 0xFFFF_FFFF) as u32 as i32;
let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
```

The buffer the host placed at `(ptr, len)` was allocated through your
`qoredb_alloc`, so its lifetime is the same as any allocation you made
yourself: valid for the rest of the call, gone when the store is reset.

## Status codes (host fns returning `i32`)

Host functions that return `i32` use these conventions:

| Code | Constant | Meaning |
| --- | --- | --- |
| `0` | `OK` | The call succeeded with no payload. |
| `-1` | `ERR_DENIED` | Capability not granted, or a secondary filter (HTTP host allowlist, FS scope) refused the request. |
| `-2` | `ERR_INVALID` | The arguments couldn't be parsed: bad pointer/length, not UTF-8, malformed URL, etc. |
| `-3` | `ERR_QUOTA` | A resource budget was exceeded (storage entries cap, oversize FS write, …). |

Any other negative code is reserved.

## Host functions (`env` imports)

Every host fn lives in the `env` namespace. All `(ptr, len)` arguments
reference the plugin's `memory` export.

### Diagnostics

```
qoredb_log(level: i32, ptr: i32, len: i32) -> i32
qoredb_notify(level: i32, ptr: i32, len: i32) -> i32
```

- `level` is `0=Debug | 1=Info | 2=Warn | 3=Error` for `log`; `0=Info | 1=Success | 2=Warning | 3=Error` for `notify`.
- Returns `OK`, `ERR_DENIED` or `ERR_INVALID`.

### Storage

```
qoredb_kv_get(key_ptr: i32, key_len: i32) -> i64         // packed, 0 if missing/denied
qoredb_kv_set(key_ptr: i32, key_len: i32,
              val_ptr: i32, val_len: i32) -> i32         // OK / ERR_DENIED / ERR_INVALID / ERR_QUOTA
qoredb_kv_del(key_ptr: i32, key_len: i32) -> i32         // OK / ERR_DENIED / ERR_INVALID
```

Caps: 256 B per key, 64 KiB per value, 1024 entries, 1 MiB total.

### Query read (`postExecute` only)

```
qoredb_query_read() -> i64    // packed, 0 if denied or no payload
```

Returns the JSON-serialised query result payload (the full `QueryResult`).
Outside `postExecute`, or when the row data is too large (>1 MiB) or
serialisation fails, returns `0`.

### Outbound HTTP

```
qoredb_http_request(method_ptr: i32, method_len: i32,
                    url_ptr: i32,    url_len: i32,
                    body_ptr: i32,   body_len: i32) -> i64   // packed JSON, 0 if denied/error
```

Returns a JSON object:

```json
{ "status": 200, "body": "..." }
```

Refusals:

- Capability `http` not granted.
- Scheme not `http`/`https`.
- Host not in `allowedHosts`.
- DNS resolves into a private/loopback/link-local/metadata address (unless
  `allowPrivateNetworks: true` in the manifest).
- Body larger than 1 MiB.
- 10 s timeout.

### Filesystem (scoped to `<plugin>/data/`)

```
qoredb_fs_read(path_ptr: i32, path_len: i32) -> i64                   // packed bytes
qoredb_fs_write(path_ptr: i32, path_len: i32,
                data_ptr: i32, data_len: i32) -> i32                  // OK / ERR_*
qoredb_fs_delete(path_ptr: i32, path_len: i32) -> i32                 // OK / ERR_*
```

The `path` is joined onto the plugin's data root; absolute paths and `..`
components are rejected. 4 MiB max per file.

### Secrets

```
qoredb_secret_get(name_ptr: i32, name_len: i32) -> i64    // packed value, 0 if denied/missing
```

The name must appear in the manifest's `runtime.capabilities.secrets` list —
a tampered consent file can't widen this.

## Capability enforcement order

For every host fn, the **first instruction of the closure** is the
capability check. A capability-denied call is therefore O(1) and observably
free of side effects (no DNS, no disk, no allocation in the guest). The
allowlists (`allowedHosts`, `secrets` names) are secondary filters applied
after the capability passes — they validate the argument, not the
permission.

See [`doc/audits/PLUGIN_CAPABILITY_CHECKS.md`](../doc/audits/PLUGIN_CAPABILITY_CHECKS.md)
for the certified ordering.

## Wall-clock and fuel budgets

Per invocation:

- **Fuel**: ≈ 50 million WASM instructions. An infinite loop traps as
  `PluginError::BudgetExceeded` once it's burned through.
- **Memory**: 256 pages (16 MiB). `memory.grow` past this traps.
- **Wall-clock**: 500 ms for `pre_execute`, 5 s for `post_execute`. The
  host treats a timeout as a failed hook (counts toward the circuit
  breaker).

Each call gets its own fresh store, so state must be persisted through
`qoredb_kv_*` if you want it to survive between invocations.

## Integrity

When the manifest carries `runtime.integrity: "sha256-<64 hex>"`, the host
computes the sha256 of the loaded `.wasm` bytes and refuses to instantiate
on mismatch. The check happens before module instantiation, so a tampered
binary never executes a single instruction.

The format is the subresource-integrity-style digest, **lowercase hex**, no
base64.

## Version negotiation

`runtime.abiVersion` must equal the host's current version (`1`). Plugins
built against a newer ABI are refused at manifest parse time with a clear
error. Older plugins (`abiVersion: 0`) are not supported — the field is
required.

## Failures the host swallows

The host runs your plugin's hooks against a defensive harness. None of the
following propagate up as a query failure:

- A trap (panic, OOB access, `unreachable`).
- A fuel-exhausted invocation.
- An ABI marshalling error (malformed JSON, packed pointer outside memory).
- A wall-clock timeout.

Each instance is logged and counts toward the per-plugin circuit breaker.
Three consecutive failures unload the plugin for the session and emit a
warning toast.
