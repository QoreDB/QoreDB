# Contributing to QoreDB

Keep contributions focused and describe the behavior they change. Coordinate
substantial new work in an issue or [Discord](https://discord.gg/Yr6P3wuZDt), unless
the maintainer has already agreed on the task. Small corrections can go straight
to a pull request.

## Start here

- [Shared working instructions](AGENTS.md): conventions for people and agents.
- [Documentation index](doc/README.md): choose a guide by task.
- [Architecture](doc/development/ARCHITECTURE.md): package boundaries and entry points.
- [Adding a driver](doc/development/DRIVERS.md) or [a command](doc/development/COMMANDS.md).

## Local setup

Use Node.js 22.12+ on the 22.x line used by CI, or a newer version supported by
the dependencies. Use the exact pnpm version in `package.json`'s `packageManager`
field. Install Rust stable through [rustup](https://rustup.rs/) and the
[Tauri system prerequisites](https://v2.tauri.app/start/prerequisites/) for your
platform. Rust edition 2024 is required; the project does not currently declare
or test a minimum supported Rust version.

```bash
git clone https://github.com/QoreDB/QoreDB
cd QoreDB
pnpm install --frozen-lockfile
pnpm tauri dev
```

`pnpm dev` starts the frontend alone; desktop IPC needs Tauri. `pnpm tauri build`
creates a production build and the repository wrapper adds `duckdb-bundled`.
The frontend `build` script runs a version-sync prebuild step. Prefer
`pnpm typecheck` for validation without producing a build.

Docker is needed only for the integration services under test. Select services
from [docker-compose.yml](docker-compose.yml), for example:

```bash
docker compose up -d postgres
```

Follow the [testing guide](doc/development/TESTING.md) for readiness checks,
required-service variables, desktop system dependencies, and headless crate tests.

## Before opening a pull request

1. Keep changes limited to the requested behavior and its tests/documentation.
2. Add the correct SPDX header to new code, including tests and extracted helpers.
   Read [licensing](doc/development/LICENSING.md) before extending a Premium module.
3. Translate new UI strings in all supported locales under `src/locales/`.
4. Update the relevant guide and `doc/FEATURES.csv` for user-visible features.
5. Run the [checks for the affected surface](doc/development/TESTING.md). Include
   actual commands/results and explain skipped runtime checks in the PR.
6. Review the diff for accidental formatting, generated files, and user changes.

Biome controls frontend style. Format only changed files; use package-scoped
Rust formatting and linting as documented in the testing guide. Comments should
explain non-obvious reasons. Follow nearby patterns rather than imposing a new
abstraction or arbitrary file-size limit.

## Security and licensing

Report vulnerabilities through [SECURITY.md](SECURITY.md), not public issues.
Core contributions use [Apache-2.0](LICENSE); Premium contributions use
[BUSL-1.1](LICENSE-BSL). Existing file headers and the
[module guidance](doc/development/LICENSING.md) determine the applicable scope.
