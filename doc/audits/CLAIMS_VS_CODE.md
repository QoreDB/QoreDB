# Public Claims vs Code Reality

> **Status:** Current
> **Original date:** 2026-07-17
> **Last reviewed:** 2026-08-02
> **Code baseline:** `bc3b03644b553e064f23355e2c8afa1f24a83a44` (originally `d785703`)
> **Review depth:** Full re-audit of nine features, read-only against the code.
> Every finding below was re-checked against the current tree on 2026-08-02 and
> carries a dated status note.
> **Scope:** Instant Data API, Sandbox mode, Migrations Manager, Notebooks, Data
> Generator, Visual Data Diff, production guardrails, the plugin system, and the
> vault/SSH/local-first surface. Every public claim about these (README, in-app
> copy, `doc/`, generated artifacts) was traced to the implementation. Not
> covered: performance claims ("~25% faster", "~15MB binary", "sub-second
> startup"), the remaining drivers, and export/backup.

## Status at a glance (2026-08-02)

| | Findings |
| --- | --- |
| **Resolved** (13) | A1, A2, B1, B2, B3, B4, B5, B9, C1, C2, C8, D2, D8 |
| **Partially resolved** (2) | C10 (migrations fixed, query path not), D1 (one of four comments) |
| **Open** (15) | B6, B7, B8, B10, B11, C3, C4, C5, C6, C7, C9, D6, D7, D9, D10 |

B9 and C8 were fixed on 2026-08-02, the day of this re-verification pass; their
notes record what shipped.

Both Critical findings are closed, and every finding that pointed at the Instant
Data API is closed with tests behind it. What remains is now entirely the category
this document exists to name: prose that outlived the code it described. **Every
open finding is a sentence, not a defect** — the two that were not (B9, C8) were
fixed the same day.

D3, D4 and D5 are not defects and carry no status — they record absent guardrails
worth knowing when the features are described. They were not re-verified in depth.

## Executive Summary

### Update, 2026-08-02

The one genuine defect is fixed. SQL Server now verifies certificates when the
user asks for verification (**A2**), and the Instant Data API — which carried
four findings — is closed on all four, with tests asserting the OpenAPI contract
so it cannot drift back silently. Two fixes came out better than this document
proposed: the migrations acknowledgement became a backend-issued token rather
than an honest client boolean (**C1**), and the Universal Query Interceptor
became universal, going from one call site to eight (**D8**) — the code caught up
with the claim instead of the claim shrinking to the code.

The July diagnosis holds for what is left. Fifteen of the seventeen open findings
are sentences: the README still says plugins execute no code while `wasmi` runs on
the query path, still describes a side-by-side diff view that is unified, still
credits Argon2 to a keychain that does not use it. `THREAT_MODEL.md` still invents
a TouchID requirement and still denies the redaction that ships. None of this
moved, which is the expected outcome — nothing in the release process makes prose
fail.

Two findings were not sentences, and both were fixed the same day. **B9** — the
Visual Data Diff capping silently at 1,000 rows — was the only one that could
mislead a user about their own data; the bound is now adjustable and announced,
including the fact that the rows come back unordered. **C8** was one line in a
dialect resolver plus the button that led to it.

**C9** remains the sharpest thing left: two built-in safety rules that do not do
what their names say, so `UPDATE users SET x=1;` still runs unconfirmed on
staging. It is a control that reads as protection and is not — it stays at the
top of the list.

### Original assessment, 2026-07-17

The engineering is, in the main, better than what is written about it. The
Migrations Manager state model is unusually careful: `check_guard` is a pure
function with 13 tests covering its truth table, the MySQL transaction-safety
list is a positive allowlist rather than a blocklist, and claim-before-script
ordering eliminates the TOCTOU window. The plugin sandbox is the strongest thing
in the codebase: metered fuel, capped memory, per-hook timeouts, a circuit
breaker, and a capability model where consent is intersected with the manifest so
that tampering with the consent file cannot widen a plugin's reach. The SQL
safety classifier works from a parsed AST with per-driver dialects and catches
data-modifying CTEs that every regex-based competitor misses.

That is not the problem. The problem is the writing.

There is one genuine security defect (**A2**, SQL Server TLS certificates are
never validated) and it is not a documentation issue — it is a code fix. Beyond
it, the pattern is consistent and worth naming, because the fix is a process not
a patch: **descriptions were written from the intended design and never revised
when the implementation landed narrower or wider.** Doc-comments describe
shutdown hooks that do not exist. An OpenAPI document advertises a response
schema the handler does not emit. `EVENTS.md` declares telemetry never
instrumented. The README says plugins execute no code, while a WASM interpreter
runs on the query path and can veto queries. The README says nothing leaves your
machine by default, while an update check fires four seconds after launch.

The drift runs in both directions. Two of the worst findings **undersell**
working code (**D6**, **A2**'s neighbours), which is its own kind of damage: the
threat model claims a TouchID requirement that does not exist while denying a
redaction feature that does.

Findings are ordered by what they cost if left alone, not by engineering effort.

## Severity Key

- **Critical** — a false public claim that can destroy user data, or a genuine
  security defect.
- **High** — a public claim is false or a shipped contract is broken; credibility
  and integration risk.
- **Medium** — gating or safety control does not hold where it is presented as
  holding.
- **Low** — internal inconsistency, invisible to users today.

---

## A. Critical

### A1. The Sandbox in-app description is false

> **Re-verified 2026-08-02** — **Resolved.** The discovery copy now describes grid staging and carries the caveat the old one lacked — "SQL editor queries are not sandboxed" (`en.json`, `license.upgrade.features.sandbox`).

**Where claimed:** `src/locales/en.json:2225-2231` (`ProDiscoveryPanel`), and the
eight other locale files carrying the same block.

```json
"sandbox": {
  "description": "Run destructive queries against a real connection without touching real data.",
  "bullet1": "Auto-rollback every change",
```

**Reality:**

- There is no query interception. `rg sandbox src/components/Query/` returns
  zero matches. A `DELETE FROM users` typed in the SQL editor executes against
  the real database immediately, sandbox active or not. The claim
  "run destructive queries without touching real data" is the exact inverse of
  the behaviour.
- There is no auto-rollback. No such mechanism exists anywhere in the sandbox
  path. Exiting sandbox mode does not even discard pending changes by default —
  `deactivateSandbox(sessionId)` keeps them (`src/lib/sandbox/sandboxStore.ts:117-130`).

**What the feature actually is:** a staging buffer for grid-level DML. Edits made
through inline editing, the row modal, grid deletes, bulk edit and the data
generator are held in `localStorage` and replayed through the driver on apply
(`src/lib/sandbox/sandboxStore.ts:14-17`, `src-tauri/src/commands/sandbox.rs:281-340`).

**Why this is Critical rather than High:** the description invites the precise
action the feature does not protect against. A user who reads "run destructive
queries against a real connection without touching real data", enables sandbox,
and runs a destructive query in the editor has been actively misled into data
loss by the product's own copy.

**Note:** `sandbox.activateHint` (`src/locales/en.json:1708`) is accurate:
"make changes locally without affecting the database". The correct string already
exists in the file; the discovery panel is the outlier.

**Fix:** rewrite `description`, `bullet1` and any sibling bullets in all nine
locales to describe grid-level staging. This is a string change, not a code
change, and it is the single highest-value item in this document.

### A2. SQL Server TLS certificates are never validated

> **Re-verified 2026-08-02** — **Resolved.** `sqlserver.rs:132-147` calls `trust_cert()` only outside `verify-ca` / `verify-full` / `verify-identity`, and wires `ssl_ca_cert` through `trust_cert_ca` for the pinned-CA case.

This one is not a documentation gap. It is a defect.

**Where:** `src-tauri/crates/qore-drivers/src/drivers/sqlserver.rs:124-129`.

```rust
tib_config.encryption(if config.ssl {
    EncryptionLevel::Required
} else {
    EncryptionLevel::NotSupported
});
tib_config.trust_cert();
```

`trust_cert()` is called **unconditionally**, on the line immediately after
encryption is set to `Required`. Enabling SSL on a SQL Server connection
therefore produces an encrypted channel to whoever answers, with no verification
that they are the server you meant. That is a textbook active MITM opening: on a
hostile network, an interceptor presenting any certificate is accepted, and the
user sees an SSL-enabled connection.

The `ssl_ca_cert` field exists on the connection config
(`qore-service/src/vault/credentials.rs:69-72`) and is honoured elsewhere, but
`build_config` never reads it. The plumbing to fix this is already present.

Severity is Critical because this is the one finding in this document where a
user doing everything right — ticking the SSL box, on a production connection —
gets a guarantee they did not receive. Everything else here is a claim that
oversells; this is a control that silently does not exist.

**Fix:** call `trust_cert()` only when the user has explicitly opted into it, and
wire `ssl_ca_cert` into `build_config` for the pinned-CA case. Until it ships, do
not describe SQL Server connections as verified or as protected against
interception.

---

## B. High

### B1. "Applied and rolled back transactionally" is false on MySQL and MariaDB

> **Re-verified 2026-08-02** — **Resolved.** `README.md:119` now qualifies the claim: "transactional rollback when the driver supports it (MySQL/MariaDB DDL is non-transactional)".

**Where claimed:** `README.md:85` (Features → Data operations → Migrations Manager).

**Reality:** `src-tauri/src/commands/migrations.rs:640-654` computes
`transaction_safe` from a positive allowlist
(`mysql_statement_is_transaction_safe`, lines 164-183). Only
`SELECT/INSERT/UPDATE/DELETE/REPLACE` qualify. All DDL, and any statement the
classifier does not recognise, forces the non-transactional path. A schema
migration on MySQL is by definition DDL, so **the nominal case runs outside a
transaction**. On failure nothing is undone; the migration is marked `failed`
(lines 684-717).

The code is right to do this and says so honestly in both the comment
("Unknown SQL is deliberately conservative so a rollback can never pretend to
undo a commit") and the error message. The UI carries a dedicated amber banner
(`src/components/Migrations/MigrationsPanel.tsx:621-639`). Only the README is
silent.

**Fix:** qualify the README claim. The showcase feature page already states this
correctly and can be reused verbatim.

### B2. "Expose saved queries as read-only REST endpoints" — there are no saved queries

> **Re-verified 2026-08-02** — **Resolved.** `README.md:129` says "parameterized SQL queries" — the wording this finding proposed.

**Where claimed:** `README.md:85` (Data quality & integration → Instant Data API),
and `instantApi.description` in the locale files.

**Reality:** `query_source` is a free-text textarea in the endpoint dialog
(`src/components/InstantApi/EndpointDialog.tsx:199-207`). There is no picker, and
no reference of any kind to the query library. The wording implies an integration
that does not exist.

**Fix:** "a parameterized SQL query" is accurate and no less appealing. Or build
the picker — it is a small change and would make the claim true.

### B3. The generated OpenAPI document does not match the handler

> **Re-verified 2026-08-02** — **Resolved.** The document emits `{data, count, truncated}`, drops `page`, and builds `servers` from the live bind address. Tests now assert all three (`openapi.rs:271-300`), so the contract cannot drift again silently.

Three separate defects in one shipped artifact. This is the worst kind of gap:
consumers generate clients from it and the clients break.

| Claim in `openapi.rs` | Reality |
| --- | --- |
| Response schema `{ data, page, total }`, `required: ["data","page"]` (`src-tauri/src/api/openapi.rs:161-169`) | Handler emits `{ data, count, truncated }` (`src-tauri/src/api/handlers.rs:439-443`) |
| `page` parameter documented, `minimum: 1, default: 1` (`openapi.rs:151-157`) | `handle_endpoint` never reads `page` (`handlers.rs:109-153`). Pagination does not exist. |
| `servers.url` hardcoded `"http://127.0.0.1:4787"` (`openapi.rs:104`) | Wrong whenever TLS is on or the port differs. The document is self-contradictory in HTTPS mode. |

The `page` contract is also asserted in the doc-comment at
`src-tauri/src/api/types.rs:33-34`.

**Fix:** either implement pagination or remove `page` and correct the response
schema. `servers.url` should be built from the live bind address. Until then,
do not describe the OpenAPI output as a usable client contract.

### B4. Instant Data API endpoints are never validated at creation

> **Re-verified 2026-08-02** — **Resolved.** `create_endpoint` calls `validate_endpoint_definition` before issuing a token, with the ordering made explicit in a comment (`instant_api.rs:198-207`).

**Where implied:** the read-only guarantee, as presented everywhere.

**Reality:** `create_endpoint` (`src-tauri/src/commands/instant_api.rs:150-178`)
never calls `analyze_sql`. An endpoint whose query is `DELETE FROM orders` saves
without complaint and fails only on the first HTTP call, with a 400. The
guarantee holds — the request-time re-analysis at `handlers.rs:138-144` is
genuine and default-deny — but the failure surfaces at the worst moment and the
UX suggests the endpoint is valid.

**Fix:** run `analyze_sql` in `create_endpoint` and reject at save time. The
function is already imported in the crate.

### B5. "Nothing leaves your machine by default" is false

> **Re-verified 2026-08-02** — **Resolved, both halves.** No analytics SDK ships at all, and `README.md:36` now names the update check outright: "The only outbound call is the GitHub update check: it never fires before you've been through onboarding, and you can switch it off."

**Where claimed:** `README.md:35`, and the local-first framing generally.

**Reality:** telemetry genuinely is opt-in and the claim holds there — PostHog
capture short-circuits unless `qoredb_analytics_enabled === 'true'`
(`src/components/Onboarding/AnalyticsService.ts:12-14`), and the onboarding
checkbox initialises to `false` (`OnboardingModal.tsx:98`). That part is clean
and worth defending.

> **Resolved since this review.** The PostHog integration was removed entirely:
> no analytics SDK is bundled, the CSP no longer allows any analytics host, and
> legacy opt-in state is purged on startup. The telemetry half of this finding is
> moot.
>
> The updater half was narrowed rather than closed: `shouldCheckUpdatesOnStartup()`
> now returns `false` until onboarding completes, and the onboarding privacy step
> names the check explicitly. A fresh install no longer contacts GitHub before the
> user has seen anything, but the check still defaults to on afterwards — which is
> a deliberate call, since an unpatched client holding production credentials is a
> worse outcome than the IP disclosure.

The updater is not. `SessionProvider.tsx:204` fires `check()` four seconds after
launch, and `shouldCheckUpdatesOnStartup()` (`:50-59`) returns `true` when no
preference is stored:

```ts
const stored = localStorage.getItem(STARTUP_PREFS_KEY);
if (!stored) return true;
```

So on a fresh install, an outbound HTTPS request to
`github.com/QoreDB/QoreDB/releases/latest/download/latest.json`
(`tauri.conf.json:26-31`) leaves the machine before the user has agreed to
anything, exposing IP, User-Agent and usage rhythm. It is disableable
(`GeneralSection.tsx:322-333`), but "by default" is precisely what the claim is
about.

This matters more than its technical severity because it is the claim most likely
to be checked by a hostile reader with Wireshark, and the one where being caught
costs the most.

**Fix:** either make the update check opt-in, or amend the claim. The honest
version is short: "No telemetry. None of your data, queries or credentials ever
leave your machine. An update check queries GitHub at startup — switch it off in
Settings."

### B6. "No code execution" is false for plugins

> **Re-verified 2026-08-02** — **Open.** `README.md:122`, `doc/FEATURES.csv:94` and `plugins/mod.rs:5-9` still say no code is executed, while `wasmi` still runs `run_pre_execute` on the query path (`commands/query.rs:217`). The drift has widened: the runtime gained capabilities and secrets modules since this was written.

**Where claimed:** `README.md:120` ("Install declarative plugins ... no code
execution"), `doc/FEATURES.csv:93` ("Déclaratif uniquement — aucune exécution de
code. WASM hooks et sandboxing à venir."), and `src-tauri/src/plugins/mod.rs:5-9`.

**Reality:** a `wasmi` interpreter executes guest WASM on the query critical path.
`WasmiRuntime` is constructed unconditionally (`plugins/runtime/manager.rs:81`),
the host is built at startup (`lib.rs:79-80`), 14 plugin commands are registered
(`lib.rs:434-448`), and `commands/query.rs:217-232` calls `run_pre_execute` —
`query.rs:233` returns `"Query blocked by plugin: {reason}"`. A plugin can veto
the user's query. There is a full settings UI, a marketplace and a consent dialog.

`plugins/mod.rs:5-9` contradicts the sibling module it declares at line 14:
`runtime/mod.rs:3` describes "the sandbox that runs plugin WASM code".

This is drift in the *undersell* direction, so it carries no user risk. It is High
anyway for two reasons. It is a **security-relevant** claim: a reader deciding
whether to install a plugin is told no code runs, which is the opposite of true,
and that is the claim they would base a trust decision on. And the showcase docs
already describe the WASM runtime accurately
(`QoreDB-showcase/content/docs/en/plugins/manifest.mdx:40-60`), so the product
currently tells two different stories about the same subsystem depending on where
you read.

**Fix:** rewrite `README.md:120`, `FEATURES.csv:93` and the `plugins/mod.rs`
header. The sandbox is good work; it deserves to be claimed.

### B7. Notebook chart cells and inter-cell references cannot be reached

> **Re-verified 2026-08-02** — **Open.** `chart` is still absent from the add-cell menu (`NotebookToolbar.tsx:265-282`), and `config.label` is still read-only in `src/` (`ChartCell.tsx:44`), so a reference still cannot resolve. `README.md:38` still advertises charts.

**Where claimed:** `README.md:137` ("Inter-cell references and Chart cells (bar,
line, pie, scatter) _[Pro]_") and `README.md:37` ("charts").

**Reality:** both are implemented and neither is reachable.

- **Chart cells:** the four chart types genuinely exist
  (`src/components/Notebook/cells/ChartCell.tsx:90-152`), but `chart` is absent
  from the Add Cell menu (`NotebookToolbar.tsx:264-285`, which offers only sql,
  markdown, contract, ai) and from the inline divider (`NotebookCellList.tsx:48-63`).
  There is also no editor for `chartConfig` anywhere — grep returns type
  definitions and one read, zero writes. Without it the component renders
  `notebook.chartNoConfig`.
- **Inter-cell references:** resolution works (`notebookInterCellRef.ts:11-33`),
  keyed on `cell.config.label`. **`config.label` is never written anywhere in
  `src/`** — two reads, zero writes. A reference cannot resolve because a label
  cannot be set.

Both require hand-editing the `.qnb` file to use. And the `_[Pro]_` tags are
fictional: `ProFeature` (`src/lib/license.ts:18-36`) contains no `chart`,
`notebook` or `inter_cell_ref` variant, and no gate exists. The BUSL-1.1 headers
on `ChartCell.tsx:1` and `notebookInterCellRef.ts:1` record the intent, but
nothing enforces it.

**Fix:** remove both from the README until they have an interface, or ship the
label field and the Add Cell entry — the rendering is already done.

### B8. Visual Data Diff: three false claims, one of them in-app

> **Re-verified 2026-08-02** — **Open.** `README.md:99` and `en.json:2311-2313` are unchanged.

**Where claimed:** `README.md:97` and `src/locales/en.json:2235-2237`.

| Claim | Reality |
| --- | --- |
| "**Side-by-side** comparison" (`README.md:97`) | The results view is **unified**, one grid (`DiffResultsGrid.tsx:64-72`); modified cells stack old (struck through) over new (`:203-213`). The only side-by-side elements are the two source *selectors*. |
| "Side-by-side **and unified views**" (`en.json:2235`) | One view exists. No toggle anywhere in `src/components/Diff/`. |
| "**Detects renames and reorders**" (`en.json:2236`) | No rename detection, for rows or columns. Matching is an exact key hash (`diffUtils.ts:33-43`). |
| "**Export the diff as SQL** or report" (`en.json:2237`) | CSV and JSON only (`DiffToolbar.tsx:119-134`). Zero SQL generation in the module. |

Three of these ship inside the product, in the Pro discovery copy — the same
surface as **A1**. A user comparing Pro tiers is reading a list of four
capabilities of which three do not exist.

### B9. Visual Data Diff silently truncates at 1,000 rows

> **Re-verified 2026-08-02** — **Open.** `useDiffSources.ts:556-560` still passes a hardcoded `1000`.
>
> **Fixed 2026-08-02.** The bound is now state (`DIFF_ROW_LIMITS`, default 1 000, adjustable to 25 000),
> a full page on either side raises a warning naming the limit and the sides affected, and changing the
> limit re-runs both sides. The warning also states that the rows come back in no guaranteed order —
> `preview_table` still has no `ORDER BY`, so a bounded diff is a prefix of an arbitrary order rather
> than of a stable one. Over-reports on a table whose row count is an exact multiple of the limit,
> which is the safe direction.

**Reality:** table-mode diffs call `previewTable(sessionId, namespace, tableName, 1000)`
— hardcoded (`useDiffSources.ts:555-561`). `QueryResult` carries no truncation
flag (`src/lib/tauri/types.ts:165-171`), the panel displays "1000 row(s)" with no
indication the table holds five million, and there is no `ORDER BY` guarantee on
what those 1,000 are.

A user comparing two large tables gets a diff of an arbitrary 1,000 rows and is
given every reason to believe it is exhaustive. This is worse than a missing
feature: it is a wrong answer presented as a complete one. Query mode has no cap,
so the workaround exists — it is just undiscoverable.

**Fix:** surface the cap in the UI and make it configurable. Until then this
feature cannot be honestly marketed, which is why it has no page.

### B10. Data Generator: "constraints honored" is largely false

> **Re-verified 2026-08-02** — **Open.** `README.md:130` still claims constraints are honoured, and `TableColumn` (`qore-core/src/types.rs:1020-1034`) still carries no length, uniqueness or CHECK information.

**Where claimed:** `README.md:113` ("Schema-aware test/seed data (types,
constraints and foreign keys honored), realistic values").

**Reality:** `TableColumn` (`qore-core/src/types.rs:757-772`) carries only `name`,
`data_type`, `nullable`, `default_value`, `is_primary_key`, `is_auto_increment`.
There is no field for max length, uniqueness, CHECK constraints, enums or domains.
The information does not exist in the structure, so it cannot be honoured.

| Claimed | Reality |
| --- | --- |
| NOT NULL | Incidental — no code reads `nullable` to decide. Actively **violated** on a NOT NULL foreign key with no parent rows: `Value::Null` is forced (`data_generator.rs:234-237`). |
| UNIQUE | Not handled. `schema.indexes[].is_unique` is never read. A non-auto-increment `int` PK draws from `1..1_000_000` with no dedup (`:324`) — at the 10,000-row maximum, collision is effectively certain. |
| CHECK | Not introspected. |
| Max length | Not handled. `user12345@example.com` into a `varchar(10)` fails. |
| Enum / domain | Not handled; falls through to the word list. |
| DEFAULT, auto-increment | Genuinely handled (column excluded, `:131`). |

"Realistic values" is 5 hardcoded constants totalling ~57 values
(`data_generator.rs:32-48`): 16 first names, 14 surnames, 10 cities, 7 countries,
10 filler words. No faker library. Seven column-name heuristics (`:264-290`).
Any unrecognised type gets `"omega_42"` (`:327`) and the INSERT fails.

Type width is also unchecked: `dt.contains("int")` matches `smallint` and
`tinyint` but generates up to 999,999.

**Fix:** the claim needs to shrink to what is true, or `TableColumn` needs the
missing fields. Honest wording: "types, defaults and foreign keys honoured".

### B11. "Native OS keychain storage (Argon2)" is misleading

> **Re-verified 2026-08-02** — **Open.** `README.md:35` and `README.md:156` are unchanged.

**Where claimed:** `README.md:153`, `README.md:204`.

**Reality:** in the default configuration Argon2 plays no part in credential
storage. Secrets go into the OS keychain as-is
(`qore-service/src/vault/storage.rs:135-137`). Argon2id is used in exactly two
places, neither of them the default path:

- hashing the app-lock master password, if the user enables it
  (`vault/lock.rs:41-50`);
- deriving the key for `vault.enc`, which only exists when `QORE_VAULT_KEY` is
  set (`vault/backend.rs:61-78`, `encrypted_file.rs:185-194`).

Both use Argon2id, 64 MiB, 3 iterations, parallelism 1 — good parameters. But
juxtaposing "keychain" and "Argon2" implies the credentials are encrypted with a
KDF, which they are not; they are protected by the OS keychain's own ACL.

Worth noting alongside: `connections.json` stores hosts, ports, usernames,
databases, SSH key paths and bastion hosts **in cleartext**
(`vault/storage.rs:20, 74-101`), with `0o600` applied on Unix only (`:92-98`) —
no permission restriction on Windows.

---

## C. Medium

### C1. Production confirmation on migrations is neutralised by the frontend

> **Re-verified 2026-08-02** — **Resolved**, and more thoroughly than this finding proposed. The acknowledgement is no longer a client boolean: the panel requests a backend-issued confirmation token (`MigrationsPanel.tsx:295-301`) which the command verifies server-side (`migrations.rs:51, 924, 1006`). A direct IPC call without a valid token is refused.

**Reality:** `src/components/Migrations/MigrationsPanel.tsx:293` and `:312` pass
`acknowledged: true` unconditionally. The backend branch
`prod_require_confirmation && !acknowledged` (`migrations.rs:575`) is therefore
unreachable from the UI. The production confirmation is a frontend dialog and
nothing more.

This leaves no middle setting: `prod_block_dangerous_sql` blocks all DDL — every
schema migration is "dangerous" by definition (`qore-sql/src/safety.rs:401-425`)
— so enabling it makes the Migrations Manager unusable in production, and
disabling it removes the only server-side guard. There is no configuration that
gives a reviewed-and-approved production migration.

**Fix:** pass the real acknowledgement state from the dialog result.

### C2. Migration driver allowlist is cosmetic

> **Re-verified 2026-08-02** — **Resolved.** `schema_migration_driver_supported` now lives in the backend (`migrations.rs:46`) and gates both entry points (`:530`, `:997`), with a test over the unsupported list.

**Reality:** `SCHEMA_MIGRATION_DRIVERS` exists only in TypeScript
(`src/lib/migrations/drivers.ts:8-20`; three usages, all frontend). The backend
enforces nothing beyond `driver.capabilities().mutations`
(`qore-service/src/mutation.rs:45-47`). A direct Tauri call against a ClickHouse
or MongoDB session passes preflight, falls through `dialect_for` to the `_ => base`
branch with every dialect flag false (`qore-sql/src/migration_split.rs:118`), and
executes SQL split under the wrong rules.

**Fix:** mirror the allowlist in `apply_migration`.

### C3. Licence gating is frontend-only across all three features

> **Re-verified 2026-08-02** — **Open**, unchanged. `license_manager` is still read only by `commands/license.rs` and by the sandbox for its Core row limit. Instant Data API and schema diff remain frontend-gated.

| Feature | Gate | Backend check |
| --- | --- | --- |
| `instant_api` | `#![cfg(feature = "pro")]` compile-time (`commands/instant_api.rs:18`) + `Sidebar.tsx:424` | None. In a Pro build the IPC is callable regardless of licence state. |
| `sandbox` | `SandboxToggle.tsx:71-74`, a toast | Only `CORE_SANDBOX_LIMIT = 5` per batch (`commands/sandbox.rs:41`) |
| `schema_diff` | `MigrationsPanel.tsx:120-122`, `AppLayout.tsx:1353` | None. `workspace_baselines.rs` is file I/O only; the diff runs entirely in the renderer. |

The BUSL-1.1 headers are correctly applied, so the licensing position is sound.
The enforcement is not. Whether this matters is a business call — for a desktop
app the frontend is not a trust boundary anyway, and a determined user can
always patch the binary. Worth a deliberate decision rather than drift.

### C4. Sandbox has no write-write conflict detection

> **Re-verified 2026-08-02** — **Open.** The apply path still infers conflicts from `affected_rows == 0` (`sandbox.rs:300, 321, 335`); no original values are captured.

**Reality:** conflict handling is partial and after the fact. `validateSandboxChanges`
(`src/components/Table/TableBrowser.tsx:353-411`) compares captured schema to live
schema — columns, types, nullability, PK — but never data. At apply,
`affected_rows == 0` produces "possible conflict"
(`src-tauri/src/commands/sandbox.rs:299, 320, 334`). If another user modified the
row in the interval, the apply overwrites it silently. A lost update.

Documented as a limit on the new feature page. Flagged here because the fix
(capture the original row values, apply with an optimistic `WHERE`) is tractable
and would turn a caveat into a selling point.

### C5. The Data Generator bypasses Sandbox mode

> **Re-verified 2026-08-02** — **Open.** `DataGeneratorDialog` still takes neither `sandboxMode` nor `onSandboxUpdate`.

**Reality:** `DataGeneratorDialog` has neither a `sandboxMode` nor an
`onSandboxUpdate` prop (`src/components/Grid/DataGeneratorDialog.tsx:24-34`), and
`DataGrid.tsx:1057-1067` passes neither — while `BulkEditDialog`, twelve lines
above, receives both (`DataGrid.tsx:1049-1050`). `handleExecute` calls
`executeQuery` directly (`DataGeneratorDialog.tsx:79`).

Clicking "Insert into table" with sandbox mode active writes to the database
immediately, with no staging and no confirmation. The user is inside a mode whose
entire premise is that writes are held back.

The trap that produces this belief is real and worth naming: `DataGeneratorDialog.tsx:9`
imports `SqlPreview` from `@/components/Sandbox/SqlPreview`. That component is a
shared, purely presentational CodeMirror wrapper under Apache-2.0 with no sandbox
logic; it merely lives in the Sandbox folder. An earlier pass of this audit
concluded from that import alone that the generator fed the sandbox, and the
error reached a published feature page before a second pass caught it. Import
paths are not evidence of behaviour.

**Fix:** pass `sandboxMode` and `onSandboxUpdate` through, or disable the
generator's execute path while sandbox mode is active. Moving `SqlPreview` to
`components/ui/` would remove the misleading signal.

### C6. `localStorage` quota is unhandled in the sandbox

> **Re-verified 2026-08-02** — **Open.** `saveSandboxState` (`sandboxStore.ts:53-55`) still has no `try`/`catch`, and `saveDraft` (`notebookIO.ts:87-93`) still swallows the quota error while serialising results.

**Reality:** `saveSandboxState` (`src/lib/sandbox/sandboxStore.ts:53-55`) has no
try/catch. The quota is roughly 5-10 MB. A `QuotaExceededError` propagates raw.
There is no cap on pending changes and no TTL on changes or backups. A bulk edit
or data generator run at volume is the realistic trigger.

Same class of bug in the notebook draft, and worse: `saveDraft`
(`src/lib/notebook/notebookIO.ts:87-93`) serialises the notebook **including
`lastResult`** — so query result data is written to `localStorage` in cleartext —
and swallows the quota error entirely (`catch { /* storage full, ignore */ }`).
The user loses their draft with no warning. `stripForSave` is applied to file
saves (`:20`) but not to drafts.

### C7. `THREAT_MODEL.md` claims a biometric control that does not exist

> **Re-verified 2026-08-02** — **Open.** `THREAT_MODEL.md:21` still claims the TouchID requirement.

**Where claimed:** `doc/security/THREAT_MODEL.md:21` — "Access requires OS-level
authentication (e.g. TouchID/Password on macOS)".

**Reality:** `rg -i "TouchID|LocalAuthentication|biometric|LAContext"` over `src`
and `src-tauri` returns nothing outside build artifacts. There is no biometric
code. On macOS the application that created a keychain item accesses it through
its own ACL without prompting.

A threat model asserting a control that does not exist is worse than one that
omits it, because it is the document a security-minded evaluator reads first, and
the one whose claims they will actually test.

**Fix:** delete the sentence.

### C8. Data Generator rejects four PostgreSQL-compatible drivers

> **Re-verified 2026-08-02** — **Open.** `from_driver_id` (`qore-sql/src/generator.rs:49-57`) still knows only the four base ids. Two drivers have been added since (Valkey, and the search engines), so the gap between this resolver and the other three has grown.
>
> **Fixed 2026-08-02.** `cockroachdb`, `cockroach`, `neon`, `supabase` and `timescaledb` now resolve to
> `Postgres`, with a test asserting they never reach the unknown-driver branch and a second asserting the
> document and key-value drivers still resolve to `None`. The UI half is closed too: the Data Generator
> button is now gated on `supportsSQL`, so it no longer appears on MongoDB, Redis or the search drivers
> only to fail after the click.

**Reality:** the generator resolves its dialect through
`qore_sql::generator::SqlDialect::from_driver_id` (`data_generator.rs:109`), which
knows only `postgres`, `postgresql`, `mysql`, `mariadb`, `sqlite`, `sqlserver`,
`mssql` (`qore-sql/src/generator.rs:49-57`). But the drivers report distinct ids:
`cockroachdb`, `neon`, `supabase`, `timescaledb`. All four are rejected with
"only supports SQL databases (Postgres, MySQL/MariaDB, SQLite, SQL Server)".

This is an omission, not a design decision: the repo's three other dialect
resolvers all handle them (`src-tauri/src/api/handlers.rs:210-211`,
`src-tauri/src/contracts/sql/dialect.rs:22-24`, `qore-query/src/dialect.rs:46`).
Only `qore-sql` was missed. The same gap causes the Sandbox's
"Unknown driver 'supabase', defaulting to PostgreSQL syntax" warning
(`generator.rs:405-411`).

The UI compounds it: the button renders whenever `mutationsSupported`
(`DataGrid.tsx:812-818`), so it is visible on MongoDB and Redis and fails only
after the user clicks Generate.

**Fix:** one line in `from_driver_id`.

### C9. Two built-in safety rules do not do what they are named

> **Re-verified 2026-08-02** — **Open.** The pattern at `safety.rs:61` is unchanged, and `is_dangerous` is still only written (`safety.rs:316`), never read by `check_rule`.

**Reality:** `builtin-confirm-update-no-where` uses the pattern
`^UPDATE\s+\S+\s+SET\s+[^;]+$` (`qore-service/src/interceptor/safety.rs:61`).

- On `UPDATE users SET name = 'x' WHERE id = 1` — `[^;]+$` happily consumes
  `name = 'x' WHERE id = 1`. **It matches.** The rule fires on UPDATEs that do
  have a WHERE, contradicting its own name and description (`:54-56`).
- On `UPDATE users SET name = 'x';` — `[^;]+` cannot consume the `;`, so `$` is
  unreachable. **It does not match.** The rule misses the exact case it exists to
  catch, defeated by a trailing semicolon.

In production the AST layer in `preflight` catches it anyway
(`qore-service/src/query.rs:337-356`). In **staging it does not**: `preflight`
only tests `is_dangerous` when `is_production`. So `UPDATE users SET x=1;` runs on
staging with no confirmation at all.

Related: `SafetyEngine.check` never reads `context.is_dangerous`
(`safety.rs:242-271`) even though the AST already computed it, and
`operation_type` comes from a first-word match (`pipeline.rs:159-177`). So
`WITH d AS (DELETE FROM users) SELECT * FROM d` classifies as `Other` and matches
no built-in rule. The engine is a keyword layer sitting on top of a good AST layer
and ignoring it.

**Fix:** wire `context.is_dangerous` into `check_rule`; it removes the first-word
weakness and both pattern bugs at once.

### C10. Production confirmation trusts a client-supplied flag

> **Re-verified 2026-08-02** — **Partially resolved.** The migrations path moved to backend-issued tokens (see **C1**). The query path did not: `acknowledged_dangerous` still travels from the renderer as a plain boolean (`commands/query.rs:116, 185, 1077`). The token store now exists, so closing this is wiring, not design.

**Reality:** `acknowledgedDangerous` travels from the renderer
(`src/lib/tauri/query.ts:65,93`) to `preflight(..., acknowledged)`
(`qore-service/src/query.rs:259`). The backend accepts it without proof. Anyone
invoking the Tauri command directly with `acknowledgedDangerous: true` bypasses
`prod_require_confirmation` entirely.

Only `prod_block_dangerous_sql` is unconditional (`query.rs:349-351`) — and it is
**off by default** (`policy.rs:66`). The typed-table-name confirmation
(`ProductionConfirmDialog.tsx:45`) is entirely cosmetic, with no backend trace.

This is the same shape as **C1** in the Migrations Manager, which hardcodes
`acknowledged: true`. Both reduce to: confirmation is a guard against haste, not
against intent. That is a defensible product position — it is just not what
"guards" implies.

---

## D. Low

### D1. Stale doc-comments

> **Re-verified 2026-08-02** — **Partially resolved.** The `openapi.rs` header now says "verified response schemas" and matches the code. Three remain stale: `auth.rs:5` still credits `OsRng` while `issue_token` uses `thread_rng()` (`:39`), `api/mod.rs:12-13` still promises app-lock and workspace-switch shutdown hooks that do not exist, and `tls.rs:33` still claims 30 days without setting `not_after`.

| Location | Says | Reality |
| --- | --- | --- |
| `src-tauri/src/api/auth.rs:5` | Tokens generated with `OsRng` | `thread_rng()` (`auth.rs:39`). `OsRng` is used only for the Argon2 salt (`auth.rs:47`). Still a CSPRNG, still safe — but do not repeat "OsRng" publicly. |
| `src-tauri/src/api/mod.rs:12-13` | Shuts down on "explicit stop, app lock, or workspace switch" | Only explicit stop exists. No app-lock or workspace-switch hook calls `stop()`. |
| `src-tauri/src/api/tls.rs:33-36` | Certificate "valid for the next 30 days" | `not_after` is never set; the value is whatever rcgen defaults to. |
| `src-tauri/src/api/openapi.rs:8` | "No response schemas" | The code emits them (`openapi.rs:161-169`). |

### D6. `THREAT_MODEL.md` denies a feature that exists

> **Re-verified 2026-08-02** — **Open.** `THREAT_MODEL.md:50` still denies the redaction that ships.

`doc/security/THREAT_MODEL.md:50` states: "**Current limitation**: The interceptor
audit/profiling pipeline currently stores **raw query text** locally."

This is stale and false. `redact_query` is called before any persistence
(`interceptor/types.rs:200`, `profiling.rs:161-166`), tests at `types.rs:381-428`,
and `doc/security/PRODUCTION_SAFETY.md:68-85` documents the redaction correctly.
The threat model undersells the product and contradicts a sibling document.

Worth pairing with **C7**: the same file invents a TouchID requirement that does
not exist and denies a redaction feature that does. It needs a pass, not a patch.

### D7. "Full retained trail" overstates the audit log

> **Re-verified 2026-08-02** — **Open.** `README.md:162` still says "full retained trail".

`README.md:159` says "JSONL/CSV export from the **full retained trail**".
Defensible on the word "retained", misleading in practice: the cache holds
`max_audit_entries`, default **10,000** (`interceptor/types.rs:323-325`), and
`maybe_rotate` (`audit.rs:146-165`) keeps the newest 75% — **7,500** — when it
trips. Older entries are deleted, not archived.

Also: the CSV export writes `query_preview`, not `query` (`export.rs:71-87`), so
CSV rows carry only the first 100 characters of each statement. JSON and JSONL
carry the full redacted text. Worth knowing before recommending CSV.

Honest wording: "JSONL/CSV export of the retained trail (10,000 rolling entries by
default, configurable)".

### D8. "Universal Query Interceptor" is not universal

> **Re-verified 2026-08-02** — **Resolved by the code rather than the copy.** `pre_execute` now runs from the mutation path, migrations, import, triggers, routines, sequences and maintenance as well as `execute_query` — eight call sites where there was one. The README claim is now substantially true; what is left is the specialised paths' own duplicated constants, which is a refactor, not a false claim.

`README.md:158`. `pre_execute` is called at exactly one site
(`qore-service/src/query.rs:388`). The specialised command paths —
`routines.rs`, `sequences.rs`, `import.rs`, `maintenance.rs`, `triggers.rs` —
each reimplement their own `READ_ONLY_BLOCKED` / `DANGEROUS_BLOCKED` constants
rather than routing through it. It is a central pipeline for `execute_query`,
duplicated elsewhere. `PRODUCTION_SAFETY.md:36` and `THREAT_MODEL.md:31` already
admit this; the README does not.

### D9. The README conflates two unrelated subsystems

> **Re-verified 2026-08-02** — **Open.** `README.md:160` and `changelog.ts:197` still conflate the two subsystems.

`README.md:157`: "Query rate limiting — Per-connection guardrail ..., **plus a
filesystem capability allow-list**."

These have nothing to do with each other. Rate limiting is
`qore-service/src/ratelimit.rs`. The capability model is
`src-tauri/src/plugins/runtime/capabilities.rs` — it governs WASM plugins, not
queries. And "allow-list" describes the wrong capability: `Fs` is a directory
scope (the plugin's own folder, `capabilities.rs:33-34`); it is `Http` that has an
allow-list (`:31-32`). `src/data/changelog.ts:165` repeats the same conflation.

**Fix:** two bullets.

### D10. The consent dialog inverts its own security model

> **Re-verified 2026-08-02** — **Open.** `ConsentDialog.tsx:107` still pre-checks every requested capability.

`ConsentDialog.tsx:107` pre-checks every capability the manifest requested
(`setGrants(new Set(initialGrants ?? requestedCaps(plugin)))`). The backend model
is carefully opt-in — consent is intersected with the manifest, checks happen at
call time, revocation is immediate — and the dialog presents it as opt-out. A user
clicking through grants everything asked for.

Given that `queryRead` + `http` together are the exfiltration path, this is the
one UI default worth reconsidering.

### D2. Declared telemetry was never instrumented

> **Re-verified 2026-08-02** — **Resolved** — see the note below, which was already recorded.

`doc/release/EVENTS.md:56-60` declares `instant_api_started`,
`instant_api_endpoint_created` and `instant_api_request` (sampled 1/100). None of
these strings exist anywhere in the codebase. Given the known opt-in measurement
gap, this is worth resolving in the direction of implementing them.

> **Resolved since this review**, in the other direction: telemetry was removed
> from the product and `EVENTS.md` moved to `doc/archive/`. No event is declared
> anymore, so none can drift.

### D3. Instant Data API: absent guardrails worth knowing

None of these are claimed anywhere, so they are not gaps in the honesty sense.
They are listed because they shape what the feature can be sold as.

- **No query timeout at all.** No `tokio::time::timeout`, no layer timeout, no
  `statement_timeout`. The only `Duration` in the module is the 2 s TLS graceful
  shutdown (`server.rs:274`). A slow query blocks until the driver returns.
- **The row cap does not protect the database.** `build_response`
  (`handlers.rs:424-447`) truncates in memory *after* full execution. No `LIMIT`
  is injected. `SELECT * FROM huge_table` on a `page_size=100` endpoint
  materialises the whole table in RAM to return 100 rows. It is a display cap.
- **No endpoint cap.** No `MAX_ENDPOINTS` constant exists.
- **`page_size` bounds are frontend-only.** 1–10000 is validated in
  `EndpointDialog.tsx:115`; the backend accepts any `u32` (`endpoints.rs:100-131`).
- **Unparameterised placeholder bug.** A non-required parameter with no default,
  absent from the query string, hits `continue` (`handlers.rs:186`), leaving
  `{{name}}` literally in the SQL and producing a 500. Masked today because the
  frontend defaults `required: true` (`EndpointDialog.tsx:99`).
- **`/openapi.json` accepts any endpoint's token** (`openapi.rs:42-44`), so one
  endpoint's token reveals the description of all others.
- **`/health` is unauthenticated** (`openapi.rs:68-74`).
- **No endpoint editing.** Changing a query means delete + recreate, which issues
  a new token.

### D4. Migrations: absent guardrails worth knowing

- **No ordering guard.** `check_guard` only inspects the current version's row
  (`migrations.rs:311-380`). Nothing prevents applying `0003` before `0001`.
  `NonMonotonic` is a lint warning, never blocking
  (`workspace_migrations.rs:131-173`).
- **No advisory locks.** No `pg_advisory_lock`, `GET_LOCK` or `sp_getapplock`.
  Cross-process safety rests on the in-database claim (`migrations.rs:788-868`).
  On MySQL DDL the claim commits immediately, so a process death between claim
  and script leaves a row marked `applied` over an unmigrated schema, with no
  `failed_at`.
- **History PK is the version string, not the parsed number.** `run.version` is
  the text (`migrations.rs:985`) while the duplicate lint compares parsed `u64`
  (`workspace_migrations.rs:131-152`). Renaming `0001_x.sql` to `001_x.sql`
  creates a second history row and the migration reappears as pending.
- **`namespace.database` is ignored on most drivers.** Honoured only by
  MySQL/MariaDB and DuckDB. Postgres acts on `schema`, which is always `None`
  here (`pg_compat.rs:297-335`); SQL Server (`sqlserver.rs:1193-1196`) and SQLite
  (`sqlite.rs:733-736`) name the parameter `_namespace`. The UI's target-database
  selector (`MigrationsPanel.tsx:481-497`) does not route anything on those
  drivers. "History table in the target database" is therefore imprecise.
- **Baselines are gitignored by design** (`workspace/manager.rs:19`). Two
  developers can hold divergent baselines and generate contradictory migrations.
  The README's phrasing — "shared through Git ... (+ schema-diff ... in Pro)" —
  invites the reading that the Pro subsystem is shared too. It is not.
- **A generated irreversible `down` is empty of executable SQL**, so rollback
  fails with "Migration has no down script" (`migrations.rs:927-929`). Intended,
  but not announced.
- **Deleting an applied migration file orphans its history row.**
  `get_migration_status` iterates over files, not history (`migrations.rs:1106-1146`),
  so the row becomes invisible.

### D5. Sandbox: absent guardrails worth knowing

- **The generated SQL is not what runs.** Apply replays each change through
  `driver.insert_row` / `update_row` / `delete_row`
  (`src-tauri/src/commands/sandbox.rs:281-340`). The script is display, copy and
  download only. Equivalent, not identical — and no test asserts the equivalence.
- **No Migrations Manager integration.** Despite "migration generation" in the
  README, there is no link to `.qoredb/migrations/`. The download is a browser
  Blob named `migration_YYYY-MM-DD.sql` (`MigrationPreview.tsx:76`). Calling it
  migration generation is generous; it is a DML script export.
- **Transactional apply defaults to off.** `use_transaction: true` is passed from
  the frontend (`TableBrowser.tsx:596`) but only takes effect if
  `driver.supports_transactions_for_session` returns true, and the trait default
  is `false` (`qore-core/src/traits.rs:494-496`). Any driver that has not
  overridden it applies changes one by one; a failure at change 4 of 10 leaves 3
  committed.
- **Dialect fallback misfires on supported drivers.** `SqlDialect::from_driver_id`
  (`qore-sql/src/generator.rs:49-57`) knows only postgres, mysql/mariadb, sqlite
  and sqlserver. `supabase`, `neon`, `timescaledb`, `cockroachdb`, `duckdb`,
  `motherduck` and `clickhouse` all fall through to
  "Unknown driver 'x', defaulting to PostgreSQL syntax" (`generator.rs:405-411`).
  For the Postgres-compatible ones the SQL is correct but the user sees an
  alarming warning. For `clickhouse` the output would be wrong.
- **No integration tests.** Six unit tests on SQL generation
  (`generator.rs:486-556`) and nothing else. The backend is stateless and receives
  changes from the frontend on every call, so the entire correctness of the
  feature rests on untested frontend code.

---

## Recommended Sequence

Rewritten 2026-08-02 for what is left. The original ordering is preserved below
for the record.

**The one that can still mislead a user, not just a reader:**

1. **C9** — wire `context.is_dangerous` into `check_rule`. Fixes both built-in
   rule bugs and the first-word classification weakness in one change, and closes
   the staging hole where `UPDATE users SET x=1;` runs unconfirmed.

*(B9 and C8, which led this list, shipped on 2026-08-02.)*

**Then the false claims, cheapest first. All strings:**

3. **B6** — the plugin "no code execution" claim, in three places
   (`README.md:122`, `FEATURES.csv:94`, `plugins/mod.rs:5-9`). The sandbox is the
   strongest thing in the codebase and is still being described as absent.
4. **B8** — remove the three non-existent Visual Diff bullets from `en.json` and
   fix "side-by-side" in the README. Still the only false claim shipping *inside*
   the product.
5. **C7, D6** — `THREAT_MODEL.md` still needs one pass, not two patches: it
   invents a control that does not exist and denies one that does.
6. **B10, B11, D7, D9** — the remaining README corrections.
7. **B7** — either remove charts and inter-cell references from the README, or
   ship the label field and the Add Cell entry. The rendering has been done for
   months; only the two inputs are missing.

**Then the code:**

8. **C10** — carry the migrations token pattern (**C1**) over to the query path.
   The store already exists; this is wiring.
9. **C6** — `saveSandboxState` still has no `try`/`catch`, and the notebook draft
    still writes query results to `localStorage` and swallows the quota error.
10. **C5** — pass `sandboxMode` through to the Data Generator, or disable its
    execute path while sandbox mode is active.
11. **D10** — stop pre-checking capabilities in the consent dialog.
12. **C4** — capture original row values and apply with an optimistic `WHERE`.
    Turns a documented caveat into a selling point.
13. **C3** — decide deliberately whether backend licence checks are wanted.
14. **D1** — three stale doc-comments left: `auth.rs:5`, `api/mod.rs:12-13`,
    `tls.rs:33`.

**Standing:** D3, D4, D5 need no action beyond being stated honestly wherever the
features are described.

<details>
<summary>Original sequence, 2026-07-17</summary>

**Ship first — one code fix, no debate:** A2. **Then the false claims, cheapest
first:** A1, B8, B5, B6, then B1/B2/B7/B10/B11, then C7/D6. **Then the code:**
C8, C9, B9, then B3/B4/C1/C2/C10, D10, C3.

</details>

The showcase feature pages already carry the honest wording for seven of the nine
features audited (`/features/instant-api`, `/features/sandbox`,
`/features/migrations`, `/features/notebooks`, `/features/production-safety`,
`/features/plugins`, plus the pre-existing three). Their "limits" sections can
serve as reference copy for the README rewrites — the work of deciding how to say
each of these things honestly is already done.

Two features have **no page and should not get one until the code moves**: the
Data Generator (B10, C8) and the Visual Data Diff (B8, B9). In both cases an
honest page would read as a defect list.

## Method

Nine independent read-only passes over the codebase at baseline
`d7857030f650e16c5fa8890b01bf1769539f377a`, each tracing public claims to
implementation. The 2026-08-02 pass re-checked each of the thirty findings
against `bc3b036`, at the file and line the finding cites, and recorded the
outcome inline. It did not look for new findings, so the scope note below still
bounds what this document covers. Every finding cites file and line. Claims that could not be traced
to code are recorded as absent rather than inferred. Numeric values quoted here
were re-verified directly against the source after the passes completed.

One correction worth recording, because it is the same failure mode this document
catalogues. An early pass reported that the Data Generator fed Sandbox mode,
inferring it from `DataGeneratorDialog.tsx:9` importing `SqlPreview` from
`@/components/Sandbox/`. That import is a shared presentational component with no
sandbox logic. The inference was wrong, it reached a published feature page, and a
later pass caught it (**C5**). Import paths are not evidence of behaviour, and an
audit is not exempt from its own standard.

Not covered: performance claims ("~25% faster on real workloads", "~15MB binary",
"sub-second startup"), the export and backup pipelines, QoreQuery, the ER diagram
— note `src/lib/license.ts:55` marks `er_diagram: 'core'` while `README.md:93`
tags it `[Pro]`, which is unresolved — and the remaining drivers. These are the
candidates for the next pass.
