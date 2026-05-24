# QoreDB plugin trust model

How to think about a plugin you're about to install — and what QoreDB
guarantees vs what it leaves to you.

## TL;DR

- **Plugins run sandboxed WebAssembly**: no implicit network, filesystem or
  query access. Every host surface is opt-in through a manifest-declared
  capability and a per-user consent grant.
- **There is no signed registry yet**: every plugin you install today comes
  from a folder *you chose*. The `Signed` badge means the plugin's author
  pinned its bytes via `runtime.integrity`; it does **not** mean QoreDB
  vetted the author.
- **Granting a capability is a real decision**: `http` is the riskiest
  (potential exfiltration), `secrets` next, then `fs` and `queryRead`. The
  consent dialog spells out the cost of each.

## Threat model in one paragraph

A plugin is third-party code running inside your QoreDB process. The
sandbox stops it from spawning processes, reading arbitrary files, opening
sockets, or peeking at memory it doesn't own — but anything you *grant* via
consent is by definition allowed. The plugin can also still trap, hang, or
return wrong answers; the runtime degrades gracefully (timeouts, circuit
breaker, per-plugin mutex), but a malicious plugin with `http` and
`queryRead` granted can absolutely exfiltrate query results to its allowed
hosts. **Capabilities are not paranoia; they're a choice.**

## What the host enforces, no matter what

These are invariants the user can't trade away:

| Invariant | Enforced where |
| --- | --- |
| WASM sandboxing (no syscalls, no host memory access) | `wasmi` runtime + `StoreLimits` |
| Fuel budget — runaway loops trap | `Config::consume_fuel(true)` + 50 M per call |
| Memory budget — `memory.grow` capped at 16 MiB | `StoreLimitsBuilder::memory_size` |
| Wall-clock timeout per hook (500 ms pre / 5 s post) | `tokio::time::timeout` in `manager.rs` |
| Circuit breaker — 3 consecutive failures = unloaded | `record_failure` / `unload_tripped` |
| Capability check first, before any side effect | `has_capability(...)` is line 1 of every host fn ([audit](../audits/PLUGIN_CAPABILITY_CHECKS.md)) |
| Capability set frozen at load time | `effective = consent ∩ requested` in `reload()` |
| HTTP allowlist re-checked vs URL at call time | `host_fns::register_http` |
| HTTP refuses private/loopback/metadata IPs (unless opt-in) | `is_private_destination` post-DNS |
| FS scoped to `<plugin>/data/`, `..` and absolute paths rejected | `scoped_fs_path` |
| Secrets gated by the manifest-declared name list | `secret_names` filter |
| Integrity check before instantiation, if declared | `verify_integrity` in `manager.rs` |
| Atomic install with rollback on failure | `install_plugin` with `.qoredb-staging` / `.qoredb-backup` |

A plugin that wants to do anything outside this list has to ask through a
capability — and that's where you come in.

## What the user decides

The **consent dialog** is the only place where trust is actually granted.
It lists every capability the manifest requested, side by side with what
the plugin will be able to do. Three rules:

1. **Skim the allowed hosts list**. `http` looks innocuous, but
   `allowedHosts: ["*"]` (when we eventually support globs) or
   `allowedHosts: ["pastebin.com"]` is a 1-way ticket for query data.
2. **Be suspicious of `allowPrivateNetworks: true`**. The consent dialog
   surfaces a warning when this flag is on. Legitimate use is rare —
   on-premise data catalogues, sidecars in a docker-compose. Anything else
   is most likely a misconfiguration or an SSRF setup.
3. **`secrets` + `http` is the worst combo**. A plugin that holds an API
   token *and* has outbound HTTP can act on your behalf. Grant both only
   to plugins you'd let into your `.env`.

You can revoke any capability later from the plugin's detail dialog. The
new consent set takes effect on the next query — no restart required.

## Signed vs unsigned

A plugin manifest may carry `runtime.integrity: "sha256-<64 hex>"`. When
it does, the host computes the sha256 of the loaded `.wasm` and refuses to
run a mismatched binary. The UI shows a `Signed` badge when this field is
present.

What `Signed` **means**:

- Whoever produced this plugin pinned the WASM bytes the manifest expects.
- A swap of the binary after publication (a tampered tarball, a
  compromised CI artefact) is caught before instantiation.

What `Signed` **does not** mean:

- That QoreDB or any third party vouches for the author.
- That the source code matches the WASM. (We can't verify that.)
- That the plugin is safe to grant capabilities to.

What `Unsigned` means:

- The manifest didn't pin the bytes. The plugin will still run, but if the
  WASM file is replaced on disk, the host won't notice.

`qoredb-plugin build` automatically writes the freshly computed sha256 into
the manifest's `integrity` field, so locally built plugins are always
signed.

## Defense in depth

Even with the worst granted set, the host keeps a few last lines:

- **Per-invocation isolation**: every hook runs in a fresh `Store` with
  fresh fuel and a fresh memory. State that survives a hook lives in the
  plugin's own KV store on disk — not in plugin memory.
- **Background dispatch for postExecute**: a slow plugin can't add
  latency to the query response (`schedule_post_execute` returns
  immediately; the actual hooks run on a tokio task under a 64-deep
  semaphore).
- **Capability denial telemetry**: every `ERR_DENIED` and every secondary
  refusal (HTTP host out of allowlist, FS escape, SSRF block) is logged
  at `warn` against the `plugins` target. `RUST_LOG=plugins=warn` and
  watch the QoreDB log for a moving line — a plugin that "tries"
  capabilities it doesn't have is worth investigating.

## What's deliberately out of scope (for now)

- **A signed registry**. There is no central catalogue and no author
  verification beyond the manifest's `author` string. Distribute plugins
  out of band.
- **Glob allowlists for HTTP**. Hosts are matched verbatim
  (case-insensitive). `*` patterns will likely arrive but aren't trusted
  yet.
- **Per-capability time/byte budgets**. There are global caps (50 M fuel,
  1 MiB queryRead payload, …) but not finer-grained ones like
  "X bytes/s outbound."

The roadmap is in [`doc/todo/PLUGINS_HARDENING.md`](../todo/PLUGINS_HARDENING.md).
