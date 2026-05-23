# QoreDB v0.1.29 — Extensibility, performance & product polish

## ✨ Highlights

This release pairs a refreshed onboarding / Pro experience with **three Core features that benefit everyone, for free**: a query result cache, a declarative plugin system, and security hardening (query rate limiting + a filesystem allow-list).

## 🎯 New features

### Query result cache [Core]

Repeated table navigation is now served instantly from a local cache.

- Caches the materialised results of the table-browse paths (`preview_table` / `query_table`).
- Cache key is the exact request (connection + namespace + table + pagination/sort/filter) — never a normalised fingerprint, so different filter values never collide.
- **Per-connection invalidation**: any mutation executed through QoreDB (SQL editor or grid edit) drops that connection's cached reads immediately.
- Bounded LRU (default 100 entries) with a configurable TTL (default 60 s) that caps staleness from mutations made outside QoreDB.
- Transparent — no behaviour change for explicit query execution (`Ctrl+Enter` always runs). Configurable in **Settings → Data & Privacy → Query result cache**: enable/disable, TTL, max entries, clear, hit-rate stats.

### Plugin System Foundation [Core]

The foundation for extending QoreDB. Two flavours ship together: declarative
contributions for users who just want to share static assets, and a sandboxed
WebAssembly runtime for plugins that need to react to the query lifecycle.

- A plugin is a folder with a `plugin.json` manifest, installed from **Settings → Plugins**.
- **Declarative contributions**: **SQL snippet packs** (wired into the editor autocomplete), **connection templates**, and **color themes** (applied as `--q-*` design tokens).
- **Executable runtime** (`wasmi`): a plugin may ship a `.wasm` module exposing a `preExecute` hook (returns `Allow` / `Warn` / `Block` per query) and/or a `postExecute` hook (observes outcome + optional row data).
- **Capability model**: four host capabilities are gated by an install-time consent dialog and revocable from the plugin detail view — `log` (write to QoreDB's log), `notify` (toast in the app), `storage` (1 MB key-value store, per plugin), `queryRead` (read row data inside `postExecute`). Each is *requested* in the manifest and *granted* by the user; revoking turns the matching host call into a no-op without restart.
- **Sandboxing**: every hook invocation runs in a fresh `Store` with a 50M-instruction fuel budget and a 16 MiB linear-memory ceiling. A runaway plugin traps without affecting the host; an errored plugin is logged and skipped, never failing the query.
- **Author SDK**: `qoredb-plugin-sdk` (Rust crate) hides the host ABI behind typed `fn(HookContext) -> Decision` and `fn(PostExecuteEnvelope)`, plus helpers for every capability (`log`, `notify`, `storage_{get,set,delete}`, `query_read_json`). Dogfooded by the bundled **SQL Linter** sample plugin that blocks `UPDATE`/`DELETE` without a `WHERE` clause.
- **UX**: executable plugins are flagged with a badge in **Settings → Plugins**, and the detail view surfaces the runtime entry, ABI version, hooks, and the granted/denied state of every capability the manifest requests.
- Manifest validation: identifier rules, version compatibility (`qoredb` requirement), theme variables restricted to the `--q-*` namespace, runtime ABI version check, and entry-filename sandboxing (no path traversal).
- Enable/disable per plugin, install/remove, edit consent, detail view of contributions and runtime.

### Security hardening [Core]

- **Query rate limiting** — a per-session token bucket (60 queries / 10 s) stops accidental runaway query loops. Generous enough never to affect human use; toggleable in **Settings → Security**.
- **Filesystem capability allow-list** — `fs:scope` now ships a positive allow-list (`$APPCONFIG`, `$APPDATA`, `$APPLOCALDATA`) instead of a deny-list only. The sensitive-path deny-list is retained as defence in depth. This closes the item deferred from v0.1.28 (`SECURITY_AUDIT.md` § 1).

### Onboarding & Pro experience

- Reworked onboarding flow, a "What's New" panel on update, and clearer Pro discovery / upgrade prompts.
- Founder badge, newsletter opt-in, and Homebrew / WinGet packaging.

## 🛠 Under the hood

- New module `src-tauri/src/cache/` (Core) — bounded LRU query result cache with TTL and per-session invalidation.
- New module `src-tauri/src/ratelimit.rs` (Core) — per-session token-bucket rate limiter.
- New module `src-tauri/src/plugins/` (Core) — manifest parsing/validation, plugin registry, and the `wasmi`-backed executable runtime with capability gating + consent persistence + per-plugin storage backend.
- New `plugins-dev/` workspace (outside `src-tauri`) — `qoredb-plugin-sdk` crate + the `qoredb.sql-linter` sample plugin built to `wasm32-unknown-unknown`.
- Plugin-to-app bridge: a tokio task drains `NotifyEvent`s and emits the `plugin-notify` Tauri event so the webview can surface a `sonner` toast.
- New commands: `get_cache_config` / `set_cache_config` / `clear_query_cache` / `get_cache_stats`, and `list_plugins` / `install_plugin` / `remove_plugin` / `set_plugin_enabled` / `get_plugin_contributions`.
- New frontend modules `src/lib/plugins/` + `src/providers/PluginProvider.tsx` + `src/components/Plugins/`.
- 23 new Rust unit tests (cache store, rate limiter, plugin manifest & registry).
- 9 locales updated (en, fr, de, es, pt-BR, ru, ja, ko, zh-CN).

## ⚠️ Known limitations

- **Query result cache** — covers the table-browse paths only; ad-hoc editor execution is never cached. Mutations made outside QoreDB are not observed — the TTL bounds that staleness.
- **Plugin system** — Phases 1–2 of the executable runtime ship together: declarative contributions, `preExecute`/`postExecute` hooks, the four Phase 2 capabilities (`log`, `notify`, `storage`, `queryRead`), and consent persistence. `queryRead` data is wired through the non-streamed success path of `execute_query` only (payload capped at 1 MiB); streaming paths and admin commands feed `postExecute` metadata without rows for now. Phase 3 capabilities (`http`, `fs`, `secrets`, `queryExec`), the AssemblyScript SDK, declarative UI contributions (`resultViewers`, `commands`) and signed-plugin distribution are tracked for later (see `doc/todo/PLUGIN_RUNTIME.md`). Plugins are installed from a local folder; no marketplace yet. Connection templates are surfaced in the plugin detail view; auto-fill into the new-connection form is planned for v0.1.30.
- **Query rate limiting** — an anti-loop guardrail (generous fixed budget), not fine-grained throughput throttling; per session, disableable in Settings.

---

**Full changelog**: `git log v0.1.28..v0.1.29`
