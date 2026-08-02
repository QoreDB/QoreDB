# QoreDB

Modern desktop database client built with Tauri 2 + React 19 + Rust.
A lightweight, fast alternative to DBeaver/pgAdmin for developers.

## Collaboration principles (read first)

These principles take precedence over speed. For a trivial task, use your judgment.

### 1. Think before coding

**Don't assume. Don't hide confusion. Surface the trade-offs.**

Before implementing:

- State your assumptions explicitly. When in doubt, ask.
- If several interpretations are possible, present them — don't choose silently.
- If a simpler approach exists, say so. Push for it when it's warranted.
- If something is unclear, stop. Name what's confusing. Ask.

### 2. Simplicity first

**The minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No unrequested "flexibility" or "configurability".
- No error handling for impossible scenarios.
- If you write 200 lines and 50 would do, rewrite.

Ask yourself: "Would a senior engineer say this is over-engineered?" If so, simplify.

### 3. Surgical changes

**Touch only what's necessary. Clean up only your own mess.**

When editing existing code:

- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor what isn't broken.
- Respect the existing style, even if you'd do it differently.
- If you spot unrelated dead code, flag it — don't delete it.

When your changes create orphans:

- Remove the imports/variables/functions that YOUR changes made unused.
- Don't delete pre-existing dead code unless explicitly asked.

The test: every changed line must trace directly back to the user's request.

### 4. Goal-driven execution

**Define success criteria. Iterate until verified.**

Turn tasks into verifiable goals:

- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Make sure the tests pass before and after"

For multi-step tasks, state a brief plan:

```text
1. [Step] → verification: [check]
2. [Step] → verification: [check]
3. [Step] → verification: [check]
```

Strong success criteria let you iterate autonomously. Weak criteria ("make it work") require constant clarification.

## Tech stack

| Layer    | Technologies                                         |
| -------- | ---------------------------------------------------- |
| Frontend | React 19, TypeScript, Vite 7, Tailwind 4, CodeMirror |
| Backend  | Rust (edition 2024), Tauri 2, SQLx, tokio            |
| Database | PostgreSQL, MySQL, MongoDB, SQLite                   |

## Project structure

```
src/                    # React/TypeScript frontend
├── components/         # UI components (Browser/, Query/, Results/, ui/)
├── hooks/              # React hooks (useTabs, useTheme, useKeyboardShortcuts)
├── lib/                # Tauri bindings, utilities, types
└── locales/            # i18n translations (en.json, fr.json)

src-tauri/              # Rust backend (Cargo workspace)
├── src/                # Tauri binary crate
│   ├── commands/       # Tauri handlers (query, mutation, export, vault)
│   └── engine/         # Glue to the engine crates
└── crates/             # Workspace crates
    ├── qore-core/      # Engine abstraction (traits.rs, types, registry, error)
    ├── qore-drivers/   # Database drivers + session manager
    ├── qore-query/     # Query AST, compiler, dialects
    ├── qore-sql/       # SQL generation, safety, connection URLs
    ├── qore-service/   # Vault, governance/policy, service context
    └── qore-{cli,mcp,server}/  # Entry-point binaries

doc/                    # Detailed documentation
├── audits/             # Security & compliance audits
├── internals/          # Internal architecture
├── private/            # Open-core notes (internal)
├── release/            # Release process & events
├── rules/              # UI/design standards & features
├── security/           # Threat model, policies
├── tests/              # Testing constraints
└── todo/               # Roadmap & upcoming specs
```

## Essential commands

```bash
pnpm install            # Install dependencies
pnpm tauri dev          # Run the app in dev (hot reload)
pnpm lint:fix           # Lint + auto-fix
pnpm format:write       # Format the code
pnpm test               # Rust tests (cargo test)
pnpm tauri build        # Production build
```

Docker for test databases: `docker-compose up -d`

## Key architecture

**Frontend → Backend**: Calls go through `src/lib/tauri.ts`, which exposes typed bindings to the Rust commands.
**Database drivers**: Each driver implements the `DataEngine` trait (`src-tauri/crates/qore-core/src/traits.rs`), lives in `qore-drivers`, and is registered in the `DriverRegistry` (qore-core) from `qore-service/src/context.rs`.
**Security**: Encrypted vault (Argon2), SQL validation before execution (`qore-sql/src/safety.rs`), sandbox mode.

## Conventions

- Reusable UI components in `src/components/ui/` (based on shadcn/Radix)
- Custom hooks prefixed with `use*` in `src/hooks/`
- Tauri commands in `src-tauri/src/commands/`, exports in `lib.rs`
- Rust errors: custom types in `engine/error.rs`, propagation with `?`

## Open Core licensing (important)

- The repo uses an **Open Core** model.
- **Core**: Apache 2.0 license (`LICENSE`)
- **Premium**: Business Source License 1.1 license (`LICENSE-BSL`)
- SPDX reference to use for Premium: `BUSL-1.1` (not `BSL-1.1`)

### Mandatory rule on code files

Every `*.ts`, `*.tsx`, `*.rs` code file must start with an SPDX header:

```ts
// SPDX-License-Identifier: Apache-2.0
```

or, for Premium files:

```ts
// SPDX-License-Identifier: BUSL-1.1
```

### Current Premium scope

The following files are currently marked Premium (`BUSL-1.1`), grouped by module:

#### AI Assistant

- `src/components/AI/*`
- `src/components/Chat/*`
- `src/components/Settings/sections/AiSection.tsx`
- `src/hooks/useAiAssistant.ts`
- `src/hooks/useAgentChat.ts`
- `src/lib/ai.ts`
- `src/lib/agent.ts`
- `src/lib/query/inlineEditDiff.ts`
- `src/providers/AiPreferencesProvider.tsx`
- `src-tauri/src/ai/*`
- `src-tauri/src/commands/ai.rs`
- `src-tauri/src/commands/agent.rs`
- `src-tauri/src/commands/chat.rs`

#### Data Contracts

- `src/components/Contracts/*`
- `src/lib/contracts/*`
- `src-tauri/src/contracts/*`
- `src-tauri/src/commands/contracts.rs`

#### Diff

- `src/components/Diff/*`
- `src/lib/diffUtils.ts`

#### Federation

- `src/components/Federation/*`
- `src/lib/connection/federation.ts`
- `src-tauri/src/federation/*`
- `src-tauri/src/commands/federation.rs`

#### Time Travel

- `src/components/TimeTravel/*`
- `src-tauri/src/time_travel/*`
- `src-tauri/src/commands/time_travel.rs`

#### Advanced Notebook

- `src/components/Notebook/cells/ChartCell.tsx`
- `src/components/Notebook/cells/ContractCell.tsx`
- `src/components/Notebook/results/CellResultSummary.tsx`
- `src/lib/notebook/notebookInterCellRef.ts`

#### Advanced Schema

- `src/components/Schema/ERDiagram.tsx`

#### Advanced Export

- `src-tauri/src/export/writers/parquet_writer.rs`
- `src-tauri/src/export/writers/xlsx.rs`

#### Profiling

- `src-tauri/src/interceptor/profiling.rs`

#### Index Suggestions

- `src/lib/query/indexSuggestions.ts`
- `src/components/Results/IndexSuggestions.tsx`

#### Schema Diff (Migrations generation, drift, Prod↔Staging)

- `src/lib/migrations/schemaDiff.ts` (+ `schemaDiff.test.ts`)
- `src/lib/migrations/schemaCompare.ts`
- `src/lib/migrations/baselineStore.ts`
- `src/components/Migrations/SchemaDeltaView.tsx`
- `src/components/Migrations/SchemaDiffViewer.tsx`
- `src-tauri/src/commands/workspace_baselines.rs`

Everything else is Core by default (`Apache-2.0`), unless explicitly decided otherwise.

### When you create/move a file

- New file: add the SPDX header at creation time.
- If a file moves from Core to Premium (or vice versa), update its SPDX header in the same commit.
- Keep consistency between the code and the root licenses (`LICENSE`, `LICENSE-BSL`).

## In-depth documentation

Consult these files depending on your task's context:

| Topic                     | File                                 |
| ------------------------- | ------------------------------------ |
| Docs index                | `doc/README.md`                      |
| Product vision            | `doc/PROJECT.md`                     |
| Features (list)           | `doc/FEATURES.csv`                   |
| Design (tokens, UX)       | `doc/rules/DESIGN.md`                |
| Database driver specifics | `doc/todo/DATABASES.md`              |
| Security / threats        | `doc/security/THREAT_MODEL.md`       |
| Security / prod           | `doc/security/PRODUCTION_SAFETY.md`  |
| Security audits           | `doc/audits/SECURITY_AUDIT.md`       |
| GDPR audits               | `doc/audits/GDPR_AUDIT.md`           |
| SSH tests                 | `doc/tests/TESTING_SSH.md`           |
| Driver limitations        | `doc/tests/DRIVER_LIMITATIONS.md`    |
| Release process           | `doc/release/RELEASE.md`             |
| Roadmap v2                | `doc/todo/v2.md`                     |
| Open-core roadmap (priv)  | `doc/private/OPEN_CORE_ROADMAP_1.md` |

## General rules

Apply internationalization systematically via `src/lib/i18n.ts`.
For translations, cover every language, and write French that is clear and concise (with accents).
Use the UI components from `src/components/ui/` as much as possible to ensure visual consistency.
When you add a new feature, remember the associated documentation (README, doc/FEATURES.csv) and the license (SPDX header).

### Code comments (anti-noise)

A comment should exist only if it explains a non-obvious **why**: rationale, gotcha, security reason, workaround, invariant, surprising behavior. Readable code needs no comment.

To avoid:

- JSDoc/comment that restates the symbol name: `/** Save sandbox state */` above `saveSandboxState()`.
- Section labels: `// Storage keys`, `// Helpers`, `// === TYPES ===`.
- Verbose file headers that repeat the file name or add meta (`Pattern follows X conventions`). At most one `//` line if the module's role isn't obvious.
- Paraphrasing the next line: `// increment i`, `// Sort results`, `// Add to beginning`.

To keep: the SPDX header (mandatory), directives (`biome-ignore`, `@ts-expect-error`), and comments that document an intent not readable from the code.

Test: if you can remove the comment without a reader losing information they wouldn't have guessed by reading the code, remove it.

### Documentation style (doc/, README)

Write sober docs, free of "AI-generated" markers: no emoji in titles, no bold titles (`# **Title**`), no export artifacts (`1\.`, `\+`), no marketing superlatives, and no phrases addressed to an agent ("What you now have…"). When a spec is delivered or a version released, move the doc to `doc/archive/` rather than leaving it lying around as if it were active.
