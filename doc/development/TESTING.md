# Testing and fast feedback

Run commands from the repository root unless stated otherwise. Choose checks by
changed behavior and package. Setup is in [CONTRIBUTING.md](../../CONTRIBUTING.md);
script definitions live in [package.json](../../package.json).

## Select the relevant checks

| Change | First checks | Broaden when needed |
| --- | --- | --- |
| Documentation or instructions | `pnpm docs:check` | Manually verify changed technical claims against source |
| Repository checker | `pnpm test:repo` and `pnpm docs:check` | Verify failure fixtures and clean-checkout behavior |
| Frontend bundle size | `pnpm test:perf`, `pnpm perf:bundle --baseline <reference.json>` | Native startup, process memory/CPU and distribution checks in the release plan |
| TypeScript utility | `pnpm test:ts path/to/file.test.ts`, `pnpm typecheck` | `pnpm test:ts` for shared behavior |
| React UI | `pnpm typecheck`, Biome on changed files | Affected tests plus manual Tauri UI flow |
| Engine types/errors | `cargo test --manifest-path src-tauri/Cargo.toml -p qore-core --lib` | Dependent crates and serialization consumers |
| SQL safety/generation | `cargo test --manifest-path src-tauri/Cargo.toml -p qore-sql --lib` | Service/protocol tests for changed policy behavior |
| Query compiler | `cargo test --manifest-path src-tauri/Cargo.toml -p qore-query --lib` | Callers for changed dialect semantics |
| Driver/service | Focused feature commands below | Real database and affected entry points |
| Desktop command | `cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib` | Integration target and actual IPC smoke test |
| Premium desktop | Relevant tests with `-p qoredb --features pro` | Core compatibility and affected release feature sets |

Check changed frontend files with `pnpm exec biome check <files>`; `pnpm check`
covers the repository's configured file types. `pnpm typecheck` runs `tsc --noEmit`.
Pass the test filename directly after `pnpm test:ts`; an extra `--` currently
causes Vitest to run the full suite instead of applying that file filter.
Vitest uses the Node environment and discovers `src/**/*.test.ts` through
[vite.config.ts](../../vite.config.ts); it does not currently run DOM component tests.

The optional [inline-edit browser check](../../scripts/test-inline-edit-ui.mjs)
mounts the real grid and pagination hook in a synthetic fixture. It needs
Playwright and Chromium installed locally. `QOREDB_PLAYWRIGHT_MODULE` may point
to an absolute Playwright module path when it is not resolvable from this repo;
`QOREDB_CHROMIUM_EXECUTABLE` may select an already installed Chromium executable.
Neither is a production dependency.

```bash
# Keep this frontend server running in another terminal.
pnpm exec vite --host 127.0.0.1 --port 1430
node scripts/test-inline-edit-ui.mjs
```

`QOREDB_UI_BASE_URL` overrides the default `http://127.0.0.1:1430`.
The fixture mocks IPC and providers inside its own browser context: it never
connects to a database, saves a connection, activates a licence or executes an
export. It checks five retained pages, scroll, selection, keyboard focus, failed
and late writes, concurrent paging and deferred dialogs. A screenshot is written
to `.perf/inline-edit-ui.png` (override with `QOREDB_UI_SCREENSHOT`). Do not run a
Vite production build concurrently: dev-server reloads would reset the fixture.
This browser check complements the pure reconciliation tests and the SQLite
driver readback test; it does not validate Tauri IPC or native WebView behavior.

The [Time Travel settings check](../../scripts/test-time-travel-settings-ui.mjs)
uses the same Vite/Playwright setup and environment variables. Run
`node scripts/test-time-travel-settings-ui.mjs` against that server. Its synthetic
fixture mounts the real settings card and checks draft-only typing, explicit
saves, pending controls, server-returned settings, failures/retries, numeric
validation, typing multiple exclusions and an unavailable feature. IPC and
licence state are simulated only inside the fixture; no application history
is read or deleted.

The [Pro workflow browser check](../../scripts/test-pro-workflows-ui.mjs) uses
the same Vite/Playwright setup. Run `node scripts/test-pro-workflows-ui.mjs`.
It checks the real diff grid's ambiguity and empty/filter states in English
and French, and deferred query-variable dialog loading with exact numeric SQL.
The library fixture uses isolated browser storage and a simulated Pro provider;
it does not execute SQL or activate a licence. Its screenshot is written to
`.perf/pro-workflows-ui.png`. It does not validate native IPC or WebView behavior.
The [diff-source lifecycle check](../../scripts/test-diff-sources-ui.mjs), run
with `node scripts/test-diff-sources-ui.mjs`, mounts the real source hook with
simulated IPC. It covers stale responses, refresh, limits, snapshot errors,
shared and late sessions, explicit retry, and workspace changes. It never opens
a database or reads application snapshots.

The [notebook lifecycle check](../../scripts/test-notebook-ui.mjs) uses the same
Vite/Playwright setup: `node scripts/test-notebook-ui.mjs`. It mounts the real
notebook hook and checks sequential references, transitive invalidation,
cancellation, context changes, undo/redo, save races, import confirmation and
reopening. IPC, native dialogs and browser file storage are simulated. It also
mounts AppOverlays with unrelated panels mocked to check deferred query-library
loading and retained search state. The pure `notebookIO.test.ts` suite separately
writes and rereads temporary QNB/HTML files using Node adapters, not Tauri plugins.

The [file-operation lifecycle check](../../scripts/test-file-operations-ui.mjs),
run with `node scripts/test-file-operations-ui.mjs`, uses the same setup. It mounts
the shared notebook-open hook, project transfer card and connection form, checking
workspace/session changes, late file reads, confirmations, duplicate actions, errors,
retry, explicit connection project IDs and cleanup of late sessions.
Native dialogs, file IO and IPC are simulated; AppLayout itself is not mounted.
The export checks also inject a rejected/deferred atomic publication and verify
that success is shown only after publication, with retry after failure.
The notebook fixture checks the previous file, dirty state and retained draft
through the same failure/retry lifecycle.

`cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib commands::file_output`
checks the atomic-file command through Tauri's mock IPC runtime with real temporary
files and production capabilities: missing grants, dialog grants, deny precedence,
replacement errors and retry. It does not run a native WebView or native dialog.
Shared failure injection and concurrent/process-exit file checks live in the
`qore-service` library tests.

The [schema-diff browser check](../../scripts/test-schema-diff-ui.mjs), run with
`node scripts/test-schema-diff-ui.mjs`, uses the same Vite/Playwright setup.
It mounts the real viewer with simulated IPC to check incomplete capture errors,
partial comparison wording, valid empty schemas and late-session cleanup. A
French error screenshot is saved to `.perf/schema-diff-incomplete.png`. No DDL or
live database is involved; the pure `schemaCapture.test.ts` checks pagination
failures, collisions and refusal of incomplete baselines separately.

`pnpm test` runs `test:ts` followed by `test:rust`. The latter runs `cargo test`
in `src-tauri`, whose root is also the `qoredb` package. It does not run all
workspace members' unit tests. Use explicit `-p` for changed crates. A workspace
run includes the vendored SQLx package and a much broader dependency/feature set.

## Frontend bundle measurements

Create a reference before changing frontend sources, then compare the candidate:

```bash
pnpm perf:bundle --output .perf/baseline.json
pnpm perf:bundle --baseline .perf/baseline.json --output .perf/candidate.json
pnpm test:perf
```

The [measurement script](../../scripts/measure-bundle.mjs) runs production Vite
directly with a manifest, replacing only `.perf/dist/`. It does not run TypeScript,
version synchronization or the native build. Reports and build outputs live under
the ignored `.perf/` directory by default. Keep the reference outside `.perf/dist/`
and explicitly pass it for comparison; without `--baseline`, the report says
`not-requested`, not that a budget passed. Archive dated evidence under `doc/tests/`
when using these results for release decisions.

Initial JS/CSS is the deduplicated static import closure of every manifest entry.
Total JS/CSS includes all emitted JS/CSS files, including those absent from the
manifest. Other resources and source maps are separate, and unreferenced files
are listed. Gzip is calculated per file at level 9 and then summed; it differs
from Vite's displayed gzip sizes and from compressing an installer or directory.

The provisional budgets are zero growth for each initial JS/CSS metric and at
most 2% growth for each total JS/CSS metric, separately in raw and gzip bytes.
The comparison exits nonzero on an exceeded budget, invalid reference or manifest,
missing emitted files, or incompatible build metadata. A zero baseline accepts
only zero; a new nonzero cost has no defined percentage and fails the budget.
Commit and dirty status identify the source context; record the actual changes
in the accompanying evidence. Toolchain, OS/architecture, Vite config, lockfile,
compression and build-environment fingerprints must match for comparison. The
JSON stores only hashes of local environment values/files, never their contents.

This is a static frontend weight check. Dynamic imports can execute immediately
at startup, and the static manifest does not establish WebView traffic, native
binary/install size, startup latency, RAM or CPU. Complete those scenarios using
the [v0.1.40 performance protocol](../todo/V0_1_40.md) before claiming release-wide
performance coverage. The frontend measurement is shared across license states
only when the frontend build is identical.

## Focused Rust features

These examples exercise SQLite without enabling every driver or the desktop:

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qore-drivers --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --no-default-features --features driver-sqlite --lib
cargo check --manifest-path src-tauri/Cargo.toml -p qore-cli --no-default-features --features driver-sqlite
```

Replace the feature with the driver being changed. Check the selected package's
manifest: service defaults include federation, and `qore-mcp` enables federation
on its service dependency even with `--no-default-features`. DuckDB can therefore
still be involved. `qoredb` directly enables all drivers. Cargo may still resolve
workspace dependencies during a focused check; this is not an offline guarantee.

For formatting/linting, use the same package and feature selection:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -p qore-core -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -p qore-core --lib -- -D warnings
```

Do not automatically format the entire workspace or clean Cargo's target tree
for an unrelated test failure. Preserve caches; diagnose the actual compiler,
linker, or runtime error first.

## Database integration evidence

[integration_databases.rs](../../src-tauri/tests/integration_databases.rs) skips
services that are unavailable unless their `QOREDB_TEST_*_REQUIRED` flag is true.
An apparently green run can therefore have no live coverage for a given driver.
For PostgreSQL, for example:

```bash
docker compose up -d postgres
docker compose exec -T postgres pg_isready -U qoredb -d testdb
QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --test integration_databases postgres_e2e -- --nocapture --test-threads=1
QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --test integration_databases time_travel_capture -- --nocapture
```

Wait for readiness before running the test. Read the `Service` mapping and
connection helpers in the test file for other services; do not derive environment
variable names from a guessed driver ID. Use disposable local databases: these
tests create/drop data. Stop only the containers started for the task and preserve
existing volumes unless removing their data was intended.

Mocks and wire-compatible stand-ins do not establish live vendor support. Record
which was used, any skipped tests, and the server/version when relevant. See
[driver limitations](../tests/DRIVER_LIMITATIONS.md),
[SSH testing](../tests/TESTING_SSH.md), and [MCP testing](../tests/TESTING_MCP.md).

## Desktop and CI constraints

- Linux desktop tests require the native packages and session-bus/keyring setup
  listed in [backend CI](../../.github/workflows/ci.yml). Headless engine tests
  avoid GTK/WebKit, but service tests can still involve native credential storage.
- Backend CI currently runs the desktop Core test selection with required
  PostgreSQL, MySQL, MongoDB, DocumentDB, Dragonfly, and PlanetScale stand-ins.
  It does not prove all workspace unit tests or all Premium paths pass.
- [Frontend CI](../../.github/workflows/frontend-lint.yml) runs the repository
  checker/tests, Biome, typecheck, and Vitest. Keep required status-check names
  stable when editing its jobs.
- `pnpm build` runs version synchronization; `pnpm tauri build` bundles native
  dependencies. Neither is the default check for a documentation or utility edit.
  Release builds also require configured signing/license inputs; consult
  [release instructions](../release/RELEASE.md).

When reporting a failure, include the command, the useful error excerpt, whether
it predates the change, and which behavior remains unverified. Successful targeted
checks are sufficient when the acceptance criteria are covered and no new risk
justifies a broader run.

### Replay report UI

With the Vite development server running, `node scripts/test-replay-ui.mjs`
checks the real report component with synthetic results, including cancellation,
coverage and French rendering, then the real Replay hook with simulated IPC for
late responses, context changes and mutation callbacks. Use the same `QOREDB_PLAYWRIGHT_MODULE`,
`QOREDB_CHROMIUM_EXECUTABLE` and `QOREDB_UI_BASE_URL` overrides as the other
browser fixtures. This is not a native Tauri test. The real PostgreSQL runner
check is `QOREDB_TEST_POSTGRES_REQUIRED=true cargo test --manifest-path
src-tauri/Cargo.toml -p qoredb --features pro --test replay_e2e -- --test-threads=1`.

`node scripts/test-replay-workspaces-ui.mjs` exercises the recording indicator
and hook with simulated IPC: workspace switch/return, stale status and previews,
late actions, exact workspace/run targets and retry after cancellation failure.
It uses the same Vite server and browser environment overrides. This does not
exercise native Tauri command dispatch or real database connections.
