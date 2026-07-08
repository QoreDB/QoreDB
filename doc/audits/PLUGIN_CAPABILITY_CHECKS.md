# Audit - Capability Check Ordering in Plugin Host Functions

> **Status:** Reviewed, spot-checked
> **Original date:** 2026-05-24
> **Last reviewed:** 2026-07-08
> **Code baseline:** `c65885d6c9032f1f8825e194fde85f8b38bfd1a1`
> **Review depth:** Freshness pass plus symbol search; not a full manual
> re-audit.
> **Scope:** `src-tauri/src/plugins/runtime/host_fns.rs`

**Criterion:** capability verification must be the very first instruction of
each host function exposed to the WASM runtime. No side effect, guest-memory
read, allocation, filesystem access, network request, or keyring read may happen
before the check.

## 2026-07-08 Freshness Review

The quick pass confirmed that `has_capability(...)` checks still exist in
`host_fns.rs` for the `Log`, `Notify`, `Storage`, `Http`, `Fs`, `Secrets`, and
`QueryRead` surfaces.

Recommended next pass: turn the ordering invariant into a regression test or
script, because the guarantee depends on the exact position of the check inside
each host-function closure.

## Why This Matters

A malicious plugin that has not been granted a capability must not trigger any
costly or observable host-side operation. If argument reads happened before the
capability check, the plugin could:

- make the host allocate guest memory before being denied;
- observe backing-store latency as a timing oracle for secret presence;
- touch disk or network paths before being rejected.

The capability-first check is the minimum guarantee that `ERR_DENIED` is
returned in O(1), with no observable side effect other than tracing.

## Audit Result

| Host fn | Capability | First instruction? | Note |
| --- | --- | --- | --- |
| `qoredb_log` | `Log` | Yes: `has_capability(&caller, CapabilityKind::Log)` | OK |
| `qoredb_notify` | `Notify` | Yes: `has_capability(&caller, CapabilityKind::Notify)` | OK |
| `qoredb_kv_get` | `Storage` | Yes: `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_kv_set` | `Storage` | Yes: `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_kv_del` | `Storage` | Yes: `has_capability(&caller, CapabilityKind::Storage)` | OK |
| `qoredb_http_request` | `Http` | Yes: `has_capability(&caller, CapabilityKind::Http)` | OK |
| `qoredb_fs_read` | `Fs` | Yes: `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_fs_write` | `Fs` | Yes: `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_fs_delete` | `Fs` | Yes: `has_capability(&caller, CapabilityKind::Fs)` | OK |
| `qoredb_secret_get` | `Secrets` | Yes: `has_capability(&caller, CapabilityKind::Secrets)` | OK |
| `qoredb_query_read` | `QueryRead` | Yes: `has_capability(&caller, CapabilityKind::QueryRead)` | OK |

**Conclusion:** every reviewed host function respects the invariant.

## Central Helper

Since Phase 4 (S4), a single helper
`has_capability(&Caller, CapabilityKind) -> bool` logs every denial through
`tracing::warn!`:

```rust
fn has_capability(caller: &Caller<'_, StoreData>, kind: CapabilityKind) -> bool {
    if caller.data().services.consent.contains(&kind) {
        return true;
    }
    tracing::warn!(
        target: "plugins",
        plugin = %caller.data().services.plugin_id,
        capability = ?kind,
        "plugin attempted to use a capability it was not granted"
    );
    false
}
```

Every host function calls
`if !has_capability(&caller, KIND) { return DENIAL; }` as the first line of its
closure body. This uniformity makes future regressions visible during diff
review: any new instruction before the `if` must trigger the question, "why
does this run before the capability check?"

## Secondary Denials

Several host functions apply a second filter after the capability check: URL to
HTTP host allowlist (`qoredb_http_request`), secret name to manifest-declared
secret list (`qoredb_secret_get`), and path to plugin-data scope
(`scoped_fs_path`). These are not capability checks. They validate arguments of
an already-authorized call, and may legitimately parse a URL or allocate local
data before rejecting the call.

## Tests

The "O(1) denial without side effects" invariant is indirectly covered by the
plugin E2E suite (`src-tauri/tests/plugins_e2e.rs`):

- `storage_capability_denied_drops_the_write`: a plugin that calls
  `qoredb_kv_set` without the capability does not touch disk.
- `http_request_to_unallowed_host_is_rejected_before_the_network`: the host
  allowlist rejects before any `reqwest` fetch.
- `fs_read_outside_the_scoped_root_is_rejected`: path scoping still holds even
  when the `Fs` capability is granted.

An ordering regression should fail these tests by creating a storage file, doing
unexpected work, or allowing a network path to proceed too far.
