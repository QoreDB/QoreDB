# Contributing to QoreDB

Thanks for considering a contribution. QoreDB is a small project maintained by one person, and clear, focused contributions move it forward fastest.

## Before you start

Open an issue or jump into the [Discord](https://discord.gg/Yr6P3wuZDt) before non-trivial work. A 5-minute chat saves a 5-hour PR going the wrong way.

For tiny changes — typos, a doc clarification, a missing translation — go straight to a PR. No issue needed.

## Project shape

```
src/                    React + TypeScript frontend (Apache-2.0 core, BUSL-1.1 premium)
src-tauri/              Rust backend, Tauri 2 commands, driver engine, vault
src-tauri/src/engine/   Database drivers (PostgreSQL, MySQL, MongoDB, …)
src-tauri/src/commands/ Tauri command handlers — the IPC surface
src/lib/                Frontend utilities and Tauri bindings (`tauri.ts`)
doc/                    Architecture, security, release process
```

Read `CLAUDE.md` at the repo root if you want the same context the maintainer uses. It captures the collaboration principles — simplicity first, surgical edits, goal-driven execution.

## Prerequisites

- **Rust** — latest stable. Install via [rustup](https://rustup.rs/).
- **Node.js** — v18+. **pnpm** is the package manager: `corepack enable && corepack prepare pnpm@latest --activate`.
- **System deps for Tauri** — follow the [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/) for your platform (libwebkit2gtk on Linux, Xcode CLT on macOS, WebView2 on Windows).
- **Docker** (optional) — used by `docker-compose.yml` to spin up local PostgreSQL, MySQL and MongoDB instances for integration testing.

## Local setup

```bash
git clone https://github.com/QoreDB/QoreDB
cd QoreDB
pnpm install
pnpm tauri dev      # boots the app with hot reload
```

For the test databases:

```bash
docker-compose up -d
```

Common scripts:

```bash
pnpm lint:fix       # Biome / ESLint with autofix
pnpm format:write   # format TS / JSON
pnpm test           # cargo test (Rust unit + integration)
pnpm tauri build    # production build
```

## Pull request checklist

1. Branch off `main`: `git checkout -b feat/<short-name>`.
2. Keep the diff focused — one PR per change. Refactors stay separate from features.
3. Add the SPDX header to new code files:
   - `// SPDX-License-Identifier: Apache-2.0` for core code
   - `// SPDX-License-Identifier: BUSL-1.1` for premium modules (see `CLAUDE.md` § Licensing for the current perimeter)
4. Update `doc/FEATURES.csv` if your change ships a user-visible feature.
5. Add or update i18n strings. The app ships **9 locales** (`src/locales/*.json`). New keys must land in `en.json` at minimum; `fr.json` is appreciated. Other locales fall back to English automatically.
6. Run `pnpm lint:fix`, `pnpm format:write`, and `pnpm test` before opening the PR.
7. Open the PR with a short summary of *why* (not just *what*) and a checklist of how you tested it.

## Adding a new database driver

If you want to add support for a database, here's the path:

1. Open an issue first — driver work is meaningful and we want to coordinate.
2. Drivers live in `src-tauri/src/engine/drivers/<your-driver>/`. Each implements the `DataEngine` trait in `src-tauri/src/engine/traits.rs`.
3. Register the driver in `DriverRegistry` (look at `src-tauri/src/engine/registry.rs`).
4. Add the frontend driver descriptor in `src/lib/connection/drivers.ts` and the logo asset in `public/databases/`.
5. Update `doc/todo/DATABASES.md` with any driver-specific quirks you discovered.
6. Add an integration test under `src-tauri/tests/` (use a Docker-provisioned instance from `docker-compose.yml` if possible).

## Code style

- TypeScript: strict mode, Biome formatter, no `any` without a comment.
- Rust: `cargo fmt`, `cargo clippy -- -D warnings`. Custom error types per module (see `src-tauri/src/engine/error.rs`).
- Components: keep files under ~300 lines. Split into smaller focused pieces if you blow past that.
- Comments: only when the *why* is non-obvious. The code should say *what* it does on its own.

## Security

Found a vulnerability? **Do not open a public issue.** See [SECURITY.md](SECURITY.md) for the private disclosure path.

## License

By contributing, you agree that:

- Contributions to **core** files are licensed under Apache-2.0.
- Contributions to **premium** files (listed in `CLAUDE.md` § Licensing) are licensed under BUSL-1.1.

If your contribution touches both, the SPDX header on each file is authoritative.
