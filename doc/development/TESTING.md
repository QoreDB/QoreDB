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
