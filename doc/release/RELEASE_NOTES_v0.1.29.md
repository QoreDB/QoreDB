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

The foundation for extending QoreDB — shipping **declarative plugins** in v0.1.29 (no code execution, no sandbox required).

- A plugin is a folder with a `plugin.json` manifest, installed from **Settings → Plugins**.
- Three contribution types: **SQL snippet packs** (wired into the editor autocomplete), **connection templates**, and **color themes** (applied as `--q-*` design tokens).
- Manifest validation: identifier rules, version compatibility (`qoredb` requirement), and theme variables restricted to the `--q-*` namespace.
- Enable/disable per plugin, install/remove, detail view of contributions.

### Security hardening [Core]

- **Query rate limiting** — a per-session token bucket (60 queries / 10 s) stops accidental runaway query loops. Generous enough never to affect human use; toggleable in **Settings → Security**.
- **Filesystem capability allow-list** — `fs:scope` now ships a positive allow-list (`$APPCONFIG`, `$APPDATA`, `$APPLOCALDATA`) instead of a deny-list only. The sensitive-path deny-list is retained as defence in depth. This closes the item deferred from v0.1.28 (`SECURITY_AUDIT.md` § 1).

### Onboarding & Pro experience

- Reworked onboarding flow, a "What's New" panel on update, and clearer Pro discovery / upgrade prompts.
- Founder badge, newsletter opt-in, and Homebrew / WinGet packaging.

## 🛠 Under the hood

- New module `src-tauri/src/cache/` (Core) — bounded LRU query result cache with TTL and per-session invalidation.
- New module `src-tauri/src/ratelimit.rs` (Core) — per-session token-bucket rate limiter.
- New module `src-tauri/src/plugins/` (Core) — manifest parsing/validation + plugin registry.
- New commands: `get_cache_config` / `set_cache_config` / `clear_query_cache` / `get_cache_stats`, and `list_plugins` / `install_plugin` / `remove_plugin` / `set_plugin_enabled` / `get_plugin_contributions`.
- New frontend modules `src/lib/plugins/` + `src/providers/PluginProvider.tsx` + `src/components/Plugins/`.
- 23 new Rust unit tests (cache store, rate limiter, plugin manifest & registry).
- 9 locales updated (en, fr, de, es, pt-BR, ru, ja, ko, zh-CN).

## ⚠️ Known limitations

- **Query result cache** — covers the table-browse paths only; ad-hoc editor execution is never cached. Mutations made outside QoreDB are not observed — the TTL bounds that staleness.
- **Plugin system** — declarative plugins only (snippet packs, connection templates, themes). No executable code, no hooks, no WASM, no sandbox, no marketplace — those are tracked for a later release. Connection templates are surfaced in the plugin detail view; auto-fill into the new-connection form is planned for v0.1.30.
- **Query rate limiting** — an anti-loop guardrail (generous fixed budget), not fine-grained throughput throttling; per session, disableable in Settings.

---

**Full changelog**: `git log v0.1.28..v0.1.29`
